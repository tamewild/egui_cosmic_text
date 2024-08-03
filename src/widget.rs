use std::hash::BuildHasher;
use std::time::Duration;

use cosmic_text::{
    Action, Attrs, Buffer, Change, Cursor, Edit, Editor, FontSystem, LayoutGlyph, Metrics, Motion,
    Selection, Shaping, SwashCache,
};
use cosmic_undo_2::{ActionIter, Commands};
use egui::{
    pos2, vec2, Color32, ColorImage, CursorIcon, Event, EventFilter, Key, NumExt, Painter, Pos2,
    Rect, Response, Sense, TextureHandle, TextureId, TextureOptions, Ui, Vec2,
};

use crate::atlas::TextureAtlas;
use crate::cursor::LineSelection;
use crate::draw::{draw_buf, draw_run};
use crate::util::{
    cursor_rect, extra_width, measure_height, measure_width_and_height, selection_rect,
};

macro_rules! public_enum {
    (
        $( #[ $main_attr:meta ] )*
        $name:ident {
            $(
                $(
                    #[ $attr:meta ]
                )*
                $variant: ident
            ),*

            $(,)?
        }
    ) => {
        $( #[ $main_attr ] )*
        #[derive(Debug, PartialEq, Copy, Clone)]
        pub enum $name {
            $(
                $( #[ $attr ])*
                $variant
            ),*
        }

        impl $name {
            #[doc(hidden)]
            pub fn variants() -> &'static [(&'static str, Self)] {
                &[
                    $(
                        (stringify!($variant), Self::$variant)
                    ),*
                ]
            }
        }
    };
}

public_enum! {
    Interactivity {
        /// The widget will respond to input and text selection
        Enabled,
        /// Widget will only respond to text selection.
        Selection,
        /// Widget is completely disabled and won't react to anything.
        ///
        /// This is if you want to disable both selection and input.
        Disabled
    }
}

impl Interactivity {
    fn sense(&self) -> Sense {
        match self {
            Self::Disabled => Sense::hover(),
            // Click is needed due to the context menu
            // We don't use egui's default drag detection either but this prevents
            // conflicting text selection and drag to scroll in a scroll area.
            _ => Sense::click_and_drag(),
        }
    }

    fn input(&self) -> bool {
        matches!(self, Interactivity::Enabled)
    }

    fn selection(&self) -> bool {
        matches!(self, Interactivity::Enabled | Interactivity::Selection)
    }
}

pub trait LayoutMode {
    /// Available size is in **physical pixels**
    fn calculate(
        &mut self,
        buf: &mut Buffer,
        font_system: &mut FontSystem,
        available_size: Vec2,
    ) -> Vec2;

    /// Some text layouts can't detect whether they should invalidate their cached state.
    /// Therefore you have to invalidate it manually.
    ///
    /// E.g: [`PureBoundingBox`] - if the text changes, you'd need to invalidate it manually.
    fn invalidate(&mut self);
}

#[derive(Default)]
pub struct PureBoundingBox(Option<Vec2>);

impl LayoutMode for PureBoundingBox {
    fn calculate(&mut self, buf: &mut Buffer, font_system: &mut FontSystem, _: Vec2) -> Vec2 {
        let sz = self.0.get_or_insert_with(|| {
            buf.set_size(font_system, None, None);
            measure_width_and_height(buf).into()
        });
        *sz
    }

    fn invalidate(&mut self) {
        self.0 = None;
    }
}

#[derive(Default)]
pub struct FillWidth {
    curr_width: f32,
    height: f32,
}

impl LayoutMode for FillWidth {
    fn calculate(
        &mut self,
        buf: &mut Buffer,
        font_system: &mut FontSystem,
        available_size: Vec2,
    ) -> Vec2 {
        if self.curr_width != available_size.x {
            self.curr_width = available_size.x;
            buf.set_size(font_system, self.curr_width.into(), None);
            self.height = measure_height(buf);
        }
        vec2(self.curr_width, self.height)
    }

    fn invalidate(&mut self) {
        self.curr_width = 0.0;
    }
}

#[derive(Default)]
pub struct FillWidthAndHeight(FillWidth);

impl LayoutMode for FillWidthAndHeight {
    fn calculate(
        &mut self,
        buf: &mut Buffer,
        font_system: &mut FontSystem,
        available_size: Vec2,
    ) -> Vec2 {
        self.0
            .calculate(buf, font_system, available_size)
            .at_least(available_size)
    }

    fn invalidate(&mut self) {
        self.0.invalidate()
    }
}

#[derive(Default)]
pub struct ShrinkToFit {
    available_width: f32,
    width: f32,
    height: f32,
}

impl LayoutMode for ShrinkToFit {
    fn calculate(
        &mut self,
        buf: &mut Buffer,
        font_system: &mut FontSystem,
        available_size: Vec2,
    ) -> Vec2 {
        if self.available_width != available_size.x {
            self.available_width = available_size.x;
            buf.set_size(font_system, self.available_width.into(), None);
            let (width, height) = measure_width_and_height(buf);
            self.width = width;
            self.height = height;
        }
        vec2(self.width, self.height)
    }

    fn invalidate(&mut self) {
        self.available_width = 0.0;
    }
}

impl LayoutMode for Box<dyn LayoutMode> {
    fn calculate(
        &mut self,
        buf: &mut Buffer,
        font_system: &mut FontSystem,
        available_size: Vec2,
    ) -> Vec2 {
        (**self).calculate(buf, font_system, available_size)
    }

    fn invalidate(&mut self) {
        (**self).invalidate()
    }
}

#[derive(Clone)]
pub struct CursorTexture {
    line_height: f32,
    texture: TextureHandle,
}

impl CursorTexture {
    pub fn new(ctx: &egui::Context, line_height: f32, color: Color32) -> Self {
        let texture = ctx.load_texture(
            "egui cosmic text cursor",
            ColorImage::new([1.0, line_height].map(|x| x as usize), color),
            TextureOptions::NEAREST,
        );
        Self {
            line_height,
            texture,
        }
    }

    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    pub fn texture_id(&self) -> TextureId {
        self.texture.id()
    }
}

#[derive(Debug, Clone)]
pub enum LineHeight {
    Absolute(f32),
    Relative(f32),
}

impl LineHeight {
    fn into_absolute(self, font_size: f32) -> f32 {
        match self {
            LineHeight::Absolute(x) => x,
            LineHeight::Relative(x) => font_size * x,
        }
    }
}

pub enum CursorStyle {
    None,
    Default(Color32),
    Texture(CursorTexture),
}

impl CursorStyle {
    fn with_texture<F: FnOnce(&CursorTexture)>(
        &mut self,
        ctx: &egui::Context,
        line_height: f32,
        f: F,
    ) {
        match self {
            CursorStyle::None => {}
            CursorStyle::Default(x) => {
                *self = CursorStyle::Texture(CursorTexture::new(ctx, line_height, *x));
                self.with_texture(ctx, line_height, f)
            }
            CursorStyle::Texture(x) => {
                f(x);
            }
        }
    }
}

impl Default for CursorStyle {
    fn default() -> Self {
        Self::Default(Color32::WHITE)
    }
}

// ???
#[derive(Clone)]
pub enum SelectionTexture {
    Default(Color32),
    Texture(TextureHandle),
}

impl SelectionTexture {
    fn with_texture<F: FnOnce(&TextureHandle)>(
        &mut self,
        ctx: &egui::Context,
        line_height: f32,
        f: F,
    ) {
        match self {
            SelectionTexture::Default(x) => {
                *self = Self::Texture(ctx.load_texture(
                    "egui cosmic text selection",
                    ColorImage::new(
                        [extra_width(line_height), line_height].map(|x| x as usize),
                        *x,
                    ),
                    TextureOptions::NEAREST,
                ));
                self.with_texture(ctx, line_height, f)
            }
            SelectionTexture::Texture(x) => f(x),
        }
    }
}

impl Default for SelectionTexture {
    fn default() -> Self {
        Self::Default(Color32::DARK_GRAY)
    }
}

#[derive(Debug, Copy, Clone)]
enum ClickType {
    Single,
    Double,
    Triple,
}

impl ClickType {
    fn promote(self) -> ClickType {
        match self {
            ClickType::Single => ClickType::Double,
            ClickType::Double => ClickType::Triple,
            ClickType::Triple => ClickType::Single,
        }
    }

    /// Takes a logical position
    fn as_action(self, pos: Pos2, pixels_per_point: f32) -> Action {
        // logical -> physical
        let Pos2 { x, y } = (pos * pixels_per_point).round();
        let [x, y] = [x as i32, y as i32];
        match self {
            ClickType::Single => Action::Click { x, y },
            ClickType::Double => Action::DoubleClick { x, y },
            ClickType::Triple => Action::TripleClick { x, y },
        }
    }
}

struct LastClick {
    time: f64,
    pos: Pos2,
    ty: ClickType,
}

macro_rules! egui_key_to_cosmic_action {
    ($name:ident; $( $key:pat => $action:expr ),* $(,)?) => {
        fn $name(key: Key) -> Option<Action> {
            match key {
                $(
                    $key => Some($action),
                )*
                _ => None
            }
        }
    };
}

macro_rules! egui_key_to_motion {
    ($( $key:pat => $action:expr ),* $(,)?) => {
        egui_key_to_cosmic_action! {
            egui_key_to_motion;
            $(
                $key => Action::Motion($action)
            ),*
        }
    };
}

egui_key_to_motion! {
    Key::ArrowLeft => Motion::Left,
    Key::ArrowRight => Motion::Right,
    Key::ArrowUp => Motion::Up,
    Key::ArrowDown => Motion::Down,
    Key::Home => Motion::Home,
    Key::End => Motion::End,
    // These operations are really slow and idk why
    //Key::PageUp => Motion::PageUp,
    //Key::PageDown => Motion::PageDown
}

egui_key_to_cosmic_action! {
    egui_key_to_non_motion;
    Key::Escape => Action::Escape,
    Key::Enter => Action::Enter,
    Key::Backspace => Action::Backspace,
    Key::Delete => Action::Delete
}

fn egui_key_to_cosmic_action(key: Key) -> Option<Action> {
    egui_key_to_motion(key).or_else(|| egui_key_to_non_motion(key))
}

fn apply_history_action_to_editor(action: cosmic_undo_2::Action<&Change>, editor: &mut Editor) {
    match action {
        cosmic_undo_2::Action::Do(x) => {
            editor.apply_change(x);
        }
        cosmic_undo_2::Action::Undo(x) => {
            let mut x = x.clone();
            x.reverse();
            editor.apply_change(&x);
        }
    }
}

#[derive(Debug, Default)]
pub struct EditorActions {
    pub scroll_to_cursor: bool,
    pub focus: bool,
}

pub trait ContextMenu {
    /// Returns whether to scroll to the cursor or not
    fn ui<L: LayoutMode>(
        self,
        ui: &mut Ui,
        editor: &mut CosmicEdit<L>,
        font_system: &mut FontSystem,
    ) -> EditorActions;

    /// Whether to show the context menu or not
    fn enabled(&self) -> bool;
}

pub struct NoContextMenu;

impl ContextMenu for NoContextMenu {
    fn ui<L: LayoutMode>(
        self,
        _: &mut Ui,
        _: &mut CosmicEdit<L>,
        _: &mut FontSystem,
    ) -> EditorActions {
        EditorActions::default()
    }

    fn enabled(&self) -> bool {
        false
    }
}

pub struct DefaultContextMenu<F: FnOnce() -> Option<String>> {
    pub read_clipboard_text: F,
}

impl<F: FnOnce() -> Option<String>> ContextMenu for DefaultContextMenu<F> {
    fn ui<L: LayoutMode>(
        self,
        ui: &mut Ui,
        editor: &mut CosmicEdit<L>,
        font_system: &mut FontSystem,
    ) -> EditorActions {
        let mut scroll_to_cursor = false;
        let mut focus = false;
        let input = editor.interactivity().input();
        if input && ui.button("Cut").clicked() && editor.cut(ui, font_system) {
            scroll_to_cursor = true;
            focus = true;
            ui.close_menu();
        }
        if ui.button("Copy").clicked() && editor.copy(ui) {
            ui.close_menu();
        }
        if input {
            if ui.button("Paste").clicked() {
                let clipboard_text = (self.read_clipboard_text)();
                if let Some(clipboard_text) = clipboard_text {
                    editor.insert_string(clipboard_text, font_system);
                    scroll_to_cursor = true;
                    focus = true;
                    ui.close_menu();
                }
            }
            ui.separator();
            if ui.button("Undo").clicked() && editor.undo() {
                scroll_to_cursor = true;
                focus = true;
                ui.close_menu();
            }
            if ui.button("Redo").clicked() && editor.redo() {
                scroll_to_cursor = true;
                focus = true;
                ui.close_menu();
            }
        }
        EditorActions {
            scroll_to_cursor,
            focus,
        }
    }

    fn enabled(&self) -> bool {
        true
    }
}

public_enum! {
    HoverStrategy {
        /// Shows the hover icon only when hovering the text's bounding box.
        BoundingBox,
        /// Shows the hover icon when hovering on the widget.
        Widget,
        /// Doesn't display a hover icon at all
        Disabled
    }
}

impl HoverStrategy {
    fn calculate_width(&self, glyphs: &[LayoutGlyph]) -> Option<f32> {
        match self {
            HoverStrategy::BoundingBox => glyphs.last().map(|x| x.x + x.w),
            _ => None,
        }
    }
}

enum ScrollState {
    Idle,
    Scrolling,
    FinishedLastFrame,
}

struct BlinkState {
    deadline: Option<f64>,
    cursor_visible: bool,
}

impl BlinkState {
    const BLINK_RATE: f64 = 0.503;

    fn new() -> Self {
        Self {
            deadline: None,
            cursor_visible: true,
        }
    }

    fn reset_time(time: f64) -> f64 {
        time + Self::BLINK_RATE
    }

    fn reset(&mut self, ctx: &egui::Context) {
        let time = Self::reset_time(ctx.input(|i| i.time));
        self.deadline = Some(time);
    }

    fn update(&mut self, ctx: &egui::Context, changed: bool) {
        let time = ctx.input(|i| i.time);

        if changed {
            self.cursor_visible = true;
            self.reset(ctx);
            return;
        }

        match self.deadline {
            None => self.reset(ctx),
            Some(deadline) if time >= deadline => {
                self.cursor_visible = !self.cursor_visible;
                self.reset(ctx);
            }
            _ => {}
        }

        ctx.request_repaint_after(Duration::from_secs_f64(Self::BLINK_RATE));
    }

    fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }
}

pub struct CosmicEdit<L: LayoutMode> {
    editor: Editor<'static>,
    interactivity: Interactivity,
    hover_strategy: HoverStrategy,
    layout_mode: L,
    cursor_style: CursorStyle,
    selection_texture: SelectionTexture,
    commands: Commands<Change>,
    last_click: Option<LastClick>,
    scroll_state: ScrollState,
    blink_state: BlinkState,
    dragging: bool,
    frame_changed: bool,
}

// TODO: Docs
impl<L: LayoutMode> CosmicEdit<L> {
    pub fn new(
        font_size: f32,
        line_height: LineHeight,
        interactivity: Interactivity,
        hover_strategy: HoverStrategy,
        layout_mode: L,
        font_system: &mut FontSystem,
    ) -> Self {
        Self {
            editor: {
                let mut editor = Editor::new(Buffer::new(
                    font_system,
                    Metrics::new(font_size, line_height.into_absolute(font_size)),
                ));
                editor.set_selection(Selection::Normal(editor.cursor()));
                editor
            },
            interactivity,
            hover_strategy,
            layout_mode,
            cursor_style: CursorStyle::default(),
            selection_texture: SelectionTexture::default(),
            commands: Commands::new(),
            last_click: None,
            scroll_state: ScrollState::Idle,
            blink_state: BlinkState::new(),
            dragging: false,
            frame_changed: false,
        }
    }

    pub fn from_editor(
        editor: Editor<'static>,
        interactivity: Interactivity,
        hover_strategy: HoverStrategy,
        layout_mode: L,
    ) -> Self {
        Self {
            editor,
            interactivity,
            hover_strategy,
            layout_mode,
            cursor_style: CursorStyle::default(),
            selection_texture: SelectionTexture::default(),
            commands: Commands::new(),
            last_click: None,
            scroll_state: ScrollState::Idle,
            blink_state: BlinkState::new(),
            dragging: false,
            frame_changed: false,
        }
    }

    pub fn with_cursor_style(mut self, style: CursorStyle) -> Self {
        self.cursor_style = style;
        self
    }

    pub fn with_selection_texture(mut self, selection_texture: SelectionTexture) -> Self {
        self.selection_texture = selection_texture;
        self
    }

    fn line_height(&self) -> f32 {
        self.editor.with_buffer(|x| x.metrics().line_height)
    }

    pub fn set_text<'a, 'b, T>(
        &mut self,
        spans: T,
        default_attrs: Attrs,
        shaping: Shaping,
        font_system: &mut FontSystem,
    ) where
        T: IntoIterator<Item = (&'a str, Attrs<'b>)>,
    {
        self.editor.with_buffer_mut(|x| {
            x.set_rich_text(font_system, spans, default_attrs, shaping);
        });
        self.invalidate_layout();
    }

    pub fn ui<S: BuildHasher + Default>(
        &mut self,
        ui: &mut Ui,
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
        atlas: &mut TextureAtlas<S>,
        context_menu: impl ContextMenu,
    ) -> Response {
        self.frame_changed = false;

        let pixels_per_point = ui.ctx().pixels_per_point();

        let base_line_height = self.line_height();

        // In physical pixels
        let size = self.editor.with_buffer_mut(|x| {
            // egui logical pixel -> physical pixel
            let (available_width, available_height) =
                (ui.available_size_before_wrap() * pixels_per_point)
                    .into();

            let sz = self.layout_mode.calculate(x, font_system, vec2(available_width, available_height));
            (sz.x, sz.y)
        });

        let (resp, mut painter) = ui.allocate_painter(
            // Size is in physical pixels -> logical pixels
            Vec2::from(size) / pixels_per_point,
            self.interactivity.sense()
        );

        let interact_pos = || {
            resp.interact_pointer_pos()
                .map(|pos| pos - resp.rect.min.to_vec2())
        };

        if self.interactivity.selection() {
            if ui.input(|i| i.pointer.primary_released()) {
                self.dragging = false;
            } else if resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed())
            {
                if !resp.lost_focus() {
                    resp.request_focus();
                }

                let interact_pos = interact_pos().unwrap();

                let curr_time = ui.input(|i| i.time);

                let click_type = if let Some(ref mut last_click) = self.last_click {
                    let diff_time = curr_time - last_click.time;
                    // https://github.com/emilk/egui/blob/114f8201709aa822a3f620404a20de2e695725ad/crates/egui/src/input_state.rs#L12
                    if diff_time < 0.5 && last_click.pos.distance(interact_pos) < 6.0 {
                        last_click.ty.promote()
                    } else {
                        ClickType::Single
                    }
                } else {
                    ClickType::Single
                };

                self.last_click = Some(LastClick {
                    time: curr_time,
                    pos: interact_pos,
                    ty: click_type,
                });

                self.change(font_system, |font_system, widget| {
                    widget
                        .editor
                        .action(font_system, click_type.as_action(interact_pos, pixels_per_point));
                });

                self.blink_state.cursor_visible = true;
                self.blink_state.reset(ui.ctx());

                self.dragging = true;
            } else if self.dragging && resp.has_focus() && resp.hovered() {
                let interact_pos = interact_pos().unwrap();

                // Let me know if this causes any problems
                let is_actual_drag = self
                    .last_click
                    .as_ref()
                    .is_some_and(|last_click| last_click.pos.distance(interact_pos) >= 6.0);

                if is_actual_drag {
                    self.change(font_system, |font_system, widget| {
                        let physical_interact_pos = (interact_pos * pixels_per_point).round();

                        widget.editor.action(
                            font_system,
                            Action::Drag {
                                x: physical_interact_pos.x as i32,
                                y: physical_interact_pos.y as i32,
                            },
                        );
                    });

                    self.blink_state.cursor_visible = true;
                    self.blink_state.reset(ui.ctx());
                }
            }
        }

        let mut should_scroll_to_cursor = false;

        if self.interactivity.input() && resp.has_focus() {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    resp.id,
                    EventFilter {
                        tab: false,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });

            let events = ui.input(|i| i.events.clone());
            for event in events {
                match event {
                    Event::Cut => {
                        if self.cut(ui, font_system) {
                            should_scroll_to_cursor = true;
                        }
                    }
                    Event::Copy => {
                        self.copy(ui);
                    }
                    Event::Paste(text) if !text.is_empty() => {
                        self.insert_string(text, font_system);
                        should_scroll_to_cursor = true;
                    }
                    Event::Key {
                        key: Key::Z,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.command => {
                        let scroll_to_cursor = match modifiers.shift {
                            true => self.redo(),
                            false => self.undo(),
                        };
                        if scroll_to_cursor {
                            should_scroll_to_cursor = true;
                        }
                    }
                    Event::Key {
                        key: Key::A,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.command => {
                        self.editor.set_cursor(Cursor::default());
                        let last_cursor = self.editor.with_buffer(|x| {
                            let line_i = x.lines.len().saturating_sub(1);
                            x.lines
                                .last()
                                .map(|x| x.text().len())
                                .map(|index| Cursor::new(line_i, index))
                                .unwrap_or_default()
                        });
                        self.editor.set_selection(Selection::Normal(last_cursor));
                    }
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if let Some(action) = egui_key_to_cosmic_action(key) {
                            self.change(font_system, |font_system, widget| {
                                if action == Action::Escape {
                                    widget.editor.set_selection(Selection::None);
                                } else if matches!(action, Action::Backspace | Action::Delete) {
                                    widget.editor.action(font_system, action);
                                    widget.invalidate_layout();
                                } else {
                                    if let Action::Motion(_) = action {
                                        widget.blink_state.cursor_visible = true;
                                        widget.blink_state.reset(ui.ctx());

                                        match widget.editor.selection() {
                                            Selection::None if modifiers.shift => {
                                                widget.editor.set_selection(Selection::Normal(
                                                    widget.editor.cursor(),
                                                ));
                                            }
                                            _ => {
                                                if !modifiers.shift {
                                                    widget.editor.set_selection(Selection::None);
                                                }
                                            }
                                        }
                                    }

                                    widget.editor.action(font_system, action);

                                    if let Action::Enter = action {
                                        widget.invalidate_layout();
                                    }
                                }
                                should_scroll_to_cursor = true;
                            });
                        }
                    }
                    Event::Text(string) => {
                        string.chars().for_each(|x| {
                            self.change(font_system, |font_system, widget| {
                                widget.editor.action(font_system, Action::Insert(x));
                            });
                        });
                        if !string.is_empty() {
                            self.invalidate_layout();
                            // Needs to be shaped to get a cursor pos
                            should_scroll_to_cursor = true;
                        }
                    }
                    _ => {}
                }
            }
        }

        if context_menu.enabled() {
            resp.context_menu(|ui| {
                let actions = context_menu.ui(ui, self, font_system);
                should_scroll_to_cursor |= actions.scroll_to_cursor;
                if actions.focus && !resp.lost_focus() {
                    resp.request_focus();
                }
            });
        }

        self.editor.shape_as_needed(font_system, false);

        if should_scroll_to_cursor {
            ui.scroll_to_rect(self.cursor_rect(resp.rect.min, pixels_per_point), None);
            self.scroll_state = ScrollState::Scrolling;
        } else if let ScrollState::Scrolling = self.scroll_state {
            let rect = self.cursor_rect(resp.rect.min, pixels_per_point);
            // This can be borked if the cursor is larger than the view, infinitely scrolling to
            // the cursor even though it's visible, though not completely.
            if ui.clip_rect().contains_rect(rect) {
                // Sometimes the clip rect can change in the next frame, probably due to how the scroll area and a frame container works.
                // E.g: { label, frame { scroll area { this widget } } }
                // This is a hack so lmk if you encounter any issues
                self.scroll_state = ScrollState::FinishedLastFrame
            } else {
                ui.scroll_to_rect(rect, None);
            }
        } else if let ScrollState::FinishedLastFrame = self.scroll_state {
            match resp.has_focus() {
                true => {
                    let rect = self.cursor_rect(resp.rect.min, pixels_per_point);
                    if ui.clip_rect().contains_rect(rect) {
                        self.scroll_state = ScrollState::Idle
                    } else {
                        ui.scroll_to_rect(rect, None);
                        self.scroll_state = ScrollState::Scrolling;
                    }
                }
                false => self.scroll_state = ScrollState::Idle,
            }
        }

        let selection_bounds = if resp.has_focus() {
            self.editor
                .selection_bounds()
                .and_then(|x @ (start, end)| (start != end).then_some(x))
        } else {
            None
        };

        if let HoverStrategy::Widget = self.hover_strategy {
            if resp.hover_pos().is_some() {
                ui.ctx().set_cursor_icon(CursorIcon::Text);
            }
        }

        self.editor.with_buffer(|x| {
            draw_buf(
                x,
                resp.rect.min,
                painter.clip_rect(),
                resp.hover_pos(),
                selection_bounds.map(|(_, end)| end),
                &mut painter,
                |x| self.hover_strategy.calculate_width(x),
                || ui.ctx().set_cursor_icon(CursorIcon::Text),
                |run| selection_bounds.and_then(|bounds| LineSelection::new(run, bounds)),
                |selection, last, painter| {
                    let rect = (selection_rect(selection, last) / pixels_per_point)
                        .translate(resp.rect.min.to_vec2());
                    self.selection_texture
                        .with_texture(ui.ctx(), base_line_height, |texture| {
                            painter.image(
                                texture.id(),
                                rect,
                                Rect::from_two_pos(Pos2::ZERO, pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        });
                },
                |run, painter| {
                    draw_run(run, font_system, swash_cache, atlas, painter, resp.rect);
                },
            )
        });

        if self.interactivity.input() && resp.has_focus() {
            self.blink_state.update(ui.ctx(), self.changed_this_frame());

            if self.blink_state.cursor_visible() {
                self.draw_cursor(ui.ctx(), &mut painter, resp.rect.min, pixels_per_point);
            }
        }

        resp
    }

    fn change<F: FnOnce(&mut FontSystem, &mut Self)>(
        &mut self,
        font_system: &mut FontSystem,
        f: F,
    ) {
        self.editor.start_change();

        f(font_system, self);

        if let Some(change) = self.editor.finish_change() {
            if !change.items.is_empty() {
                self.commands.push(change);
                self.frame_changed = true;
            }
        }
    }

    /// Returns whether to scroll to cursor
    fn apply_history_actions(
        &mut self,
        actions: impl FnOnce(&mut Commands<Change>) -> ActionIter<Change>,
    ) -> bool {
        let mut changed = false;
        actions(&mut self.commands).for_each(|x| {
            apply_history_action_to_editor(x, &mut self.editor);
            changed = true;
        });
        if changed {
            self.invalidate_layout();
            self.editor.set_selection(Selection::None);
        }
        changed
    }

    pub fn undo(&mut self) -> bool {
        self.apply_history_actions(Commands::undo)
    }

    pub fn redo(&mut self) -> bool {
        self.apply_history_actions(Commands::redo)
    }

    pub fn copy(&mut self, ui: &mut Ui) -> bool {
        if self
            .editor
            .selection_bounds()
            .is_some_and(|(start, end)| start == end)
        {
            return false;
        }
        if let Some(string) = self.editor.copy_selection() {
            ui.output_mut(|x| x.copied_text = string);
            return true;
        }
        false
    }

    pub fn cut(&mut self, ui: &mut Ui, font_system: &mut FontSystem) -> bool {
        if !self.copy(ui) {
            return false;
        }
        self.change(font_system, |_font_system, widget| {
            widget.editor.delete_selection();
        });
        true
    }

    // Check if string is empty here?
    pub fn insert_string(&mut self, string: String, font_system: &mut FontSystem) {
        debug_assert!(!string.is_empty());
        self.change(font_system, |_font_system, widget| {
            widget.editor.insert_string(string.as_str(), None);
        });
        self.invalidate_layout();
    }

    pub fn invalidate_layout(&mut self) {
        self.layout_mode.invalidate();
    }

    // Batch with buffer size?
    pub fn set_font_size(
        &mut self,
        font_size: f32,
        line_height: LineHeight,
        font_system: &mut FontSystem,
    ) {
        let metrics = Metrics::new(font_size, line_height.into_absolute(font_size));
        self.editor.with_buffer_mut(|x| {
            if x.metrics() != metrics {
                x.set_metrics(font_system, metrics);
                self.layout_mode.invalidate();
            }
        });
    }

    pub fn text(&self) -> String {
        self.editor.with_buffer(|x| {
            x.lines.iter().fold(String::new(), |mut str, line| {
                str.push_str(line.text());
                str.push('\n');
                str
            })
        })
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn into_editor(self) -> Editor<'static> {
        self.editor
    }

    pub fn interactivity(&self) -> Interactivity {
        self.interactivity
    }

    pub fn interactivity_mut(&mut self) -> &mut Interactivity {
        &mut self.interactivity
    }

    pub fn hover_strategy(&self) -> HoverStrategy {
        self.hover_strategy
    }

    pub fn hover_strategy_mut(&mut self) -> &mut HoverStrategy {
        &mut self.hover_strategy
    }

    pub fn set_layout_mode(&mut self, layout_mode: L) {
        self.layout_mode = layout_mode;
        self.layout_mode.invalidate();
    }

    /// Was the buffer's text changed this frame through user input?
    pub fn changed_this_frame(&self) -> bool {
        self.frame_changed
    }

    /// Returns the cursor rect in **logical pixels**
    pub fn cursor_rect(&self, logical_min_pos: Pos2, pixels_per_point: f32) -> Rect {
        let cursor = self.editor.cursor();
        self.editor
            .with_buffer(|x| {
                (cursor_rect(x, cursor).unwrap() / pixels_per_point)
                    .translate(logical_min_pos.to_vec2())
            })
    }

    fn draw_cursor(
        &mut self,
        ctx: &egui::Context,
        painter: &mut Painter,
        logical_min_pos: Pos2,
        pixels_per_point: f32
    ) {
        // Probably shouldn't render the cursor if it isn't in view.
        // Shouldn't matter much, it'll be clipped, etc.
        let cursor_rect = painter.round_rect_to_pixels(self.cursor_rect(logical_min_pos, pixels_per_point));
        self.cursor_style
            .with_texture(ctx, self.line_height(), |cursor_texture| {
                let cursor_texture_id = cursor_texture.texture_id();
                painter.image(
                    cursor_texture_id,
                    cursor_rect,
                    Rect::from_two_pos(Pos2::ZERO, pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            });
    }
}
