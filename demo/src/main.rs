use std::hash::BuildHasherDefault;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use arboard::Clipboard;
use eframe::egui::load::SizedTexture;
use eframe::egui::style::ScrollStyle;
use eframe::egui::{
    CentralPanel, Color32, ComboBox, Context, FontData, FontDefinitions, Frame, Margin, ScrollArea,
    SidePanel, Slider, TopBottomPanel, Vec2, Widget, WidgetText, Window,
};
use eframe::epaint::FontFamily;
#[cfg(not(target_arch = "wasm32"))]
use eframe::NativeOptions;
#[cfg(target_arch = "wasm32")]
use eframe::WebOptions;
use eframe::{App, AppCreator};
#[cfg(target_arch = "wasm32")]
use log::LevelFilter;
use rustc_hash::FxHasher;

use egui_cosmic_text::atlas::TextureAtlas;
use egui_cosmic_text::cosmic_text;
use egui_cosmic_text::cosmic_text::fontdb::Source;
use egui_cosmic_text::cosmic_text::{
    Attrs, Family, FontSystem, Metrics, Shaping, SwashCache, Weight,
};
use egui_cosmic_text::widget::{
    CosmicEdit, DefaultContextMenu, FillWidth, FillWidthAndHeight, HoverStrategy, Interactivity,
    LayoutMode, LineHeight, PureBoundingBox, ShrinkToFit,
};

#[derive(Debug, PartialEq, Default, Copy, Clone)]
enum SelectedLayoutMode {
    FillWidth,
    #[default]
    FillWidthAndHeight,
    PureBoundingBox,
    ShrinkToFit,
}

impl SelectedLayoutMode {
    fn into_layout_mode(self) -> Box<dyn LayoutMode> {
        match self {
            SelectedLayoutMode::FillWidth => Box::<FillWidth>::default(),
            SelectedLayoutMode::FillWidthAndHeight => Box::<FillWidthAndHeight>::default(),
            SelectedLayoutMode::PureBoundingBox => Box::<PureBoundingBox>::default(),
            SelectedLayoutMode::ShrinkToFit => Box::<ShrinkToFit>::default(),
        }
    }
}

struct DemoApp {
    font_system: FontSystem,
    swash_cache: SwashCache,
    texture_atlas: TextureAtlas<BuildHasherDefault<FxHasher>>,
    editor: CosmicEdit<Box<dyn LayoutMode>>,
    bottom_text: CosmicEdit<PureBoundingBox>,
    #[cfg(not(target_arch = "wasm32"))]
    clipboard: Clipboard,
    font_size: f32,
    rel_line_height: f32,
    selected_layout_mode: SelectedLayoutMode,
    updated_max_texture_side: bool,
    show_texture_atlas: bool,
}

impl App for DemoApp {
    fn update(&mut self, ctx: &Context, _: &mut eframe::Frame) {
        if !self.updated_max_texture_side {
            self.texture_atlas.update_max_texture_side();
            self.updated_max_texture_side = true;
        }

        let mut curr_layout_mode = self.selected_layout_mode;

        SidePanel::left("side_bar")
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(WidgetText::from("Controls").heading().strong());

                let button_text = match self.show_texture_atlas {
                    true => "Hide Texture Atlas",
                    false => "Show Texture Atlas"
                };

                if ui.button(button_text).clicked() {
                    self.show_texture_atlas = !self.show_texture_atlas;
                }

                if self.show_texture_atlas {
                    Window::new("Texture Atlas")
                        .open(&mut self.show_texture_atlas)
                        .collapsible(false)
                        .show(ui.ctx(), |ui| {
                            let size @ Vec2 { x, y } = self.texture_atlas.atlas_texture_size();
                            let max_texture_side = ui.input(|i| i.max_texture_side);
                            ui.label(format!("Atlas size: {x} x {y} • Max texture side: {max_texture_side}"));

                            ScrollArea::both()
                                .show(ui, |ui| {
                                    ui.image(
                                        SizedTexture::new(
                                            self.texture_atlas.atlas_texture(),
                                            size
                                        )
                                    );
                                });
                        });
                }

                ui.label("Font Size");
                Slider::new(&mut self.font_size, 5.0..=150.0)
                    .ui(ui);

                ui.label("Relative Line Height");
                Slider::new(&mut self.rel_line_height, 1.0..=3.0)
                    .ui(ui);

                ui.label("Interactivity");

                let interactivity = self.editor.interactivity_mut();

                ComboBox::from_id_source("interactivity")
                    .selected_text(format!("{interactivity:?}"))
                    .show_ui(ui, |ui| {
                        for (name, variant) in Interactivity::variants() {
                            ui.selectable_value(interactivity, *variant, &**name);
                        }
                    });

                ui.label("Hover Strategy");

                let hover_strategy = self.editor.hover_strategy_mut();

                ComboBox::from_id_source("hover_strategy")
                    .selected_text(format!("{hover_strategy:?}"))
                    .show_ui(ui, |ui| {
                        for (name, variant) in HoverStrategy::variants() {
                            ui.selectable_value(hover_strategy, *variant, &**name);
                        }
                    });

                ui.label("Layout Mode");

                ComboBox::from_id_source("layout_mode")
                    .selected_text(format!("{curr_layout_mode:?}"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut curr_layout_mode,
                            SelectedLayoutMode::FillWidth,
                            "FillWidth"
                        ).on_hover_text("Fills the width of the available space. Height is the raw height of the text.");
                        ui.selectable_value(
                            &mut curr_layout_mode,
                            SelectedLayoutMode::FillWidthAndHeight,
                            "FillWidthAndHeight"
                        ).on_hover_text("Fills the width of the available space. Minimum height will be the available space, if the raw height of the text is higher, it will be raw height of the text.");
                        ui.selectable_value(
                            &mut curr_layout_mode,
                            SelectedLayoutMode::PureBoundingBox,
                            "PureBoundingBox"
                        ).on_hover_text("Size will be the raw size of the text");
                        ui.selectable_value(
                            &mut curr_layout_mode,
                            SelectedLayoutMode::ShrinkToFit,
                            "ShrinkToFit"
                        ).on_hover_text("Shrinks to the text's width, caps out at the available width. Height is the raw height of the text.");
                    })
            });

        TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                self.bottom_text.ui(
                    ui,
                    &mut self.font_system,
                    &mut self.swash_cache,
                    &mut self.texture_atlas,
                    DefaultContextMenu {
                        #[cfg(not(target_arch = "wasm32"))]
                        read_clipboard_text: || self.clipboard.get_text().ok(),
                        #[cfg(target_arch = "wasm32")]
                        read_clipboard_text: || None,
                    },
                )
            });
        });

        CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(Color32::BLACK)
                    .inner_margin(Margin::same(5.0)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().scroll = ScrollStyle::solid();

                ScrollArea::vertical().show(ui, |ui| {
                    Frame::side_top_panel(ui.style())
                        .inner_margin(Margin::same(5.0))
                        .show(ui, |ui| {
                            self.editor.set_font_size(
                                self.font_size,
                                LineHeight::Relative(self.rel_line_height),
                                &mut self.font_system,
                            );

                            if self.selected_layout_mode != curr_layout_mode {
                                self.selected_layout_mode = curr_layout_mode;
                                self.editor
                                    .set_layout_mode(curr_layout_mode.into_layout_mode());
                            }

                            self.editor.ui(
                                ui,
                                &mut self.font_system,
                                &mut self.swash_cache,
                                &mut self.texture_atlas,
                                DefaultContextMenu {
                                    #[cfg(not(target_arch = "wasm32"))]
                                    read_clipboard_text: || self.clipboard.get_text().ok(),
                                    #[cfg(target_arch = "wasm32")]
                                    read_clipboard_text: || None,
                                },
                            );
                        });
                });
            });

        self.texture_atlas.trim();
        self.font_system.shape_run_cache.trim(1024);
    }
}

fn app_creator() -> AppCreator {
    let mut font_db = fontdb::Database::new();

    let font_file = include_bytes!("../resources/Ubuntu-Light.ttf");

    font_db.load_font_source(Source::Binary(Arc::new(font_file)));

    font_db.load_font_source(Source::Binary(Arc::new(include_bytes!(
        "../resources/TwemojiCOLRv0.ttf"
    ))));

    let mut font_system = FontSystem::new_with_locale_and_db("en-US".to_string(), font_db);
    let swash_cache = SwashCache::new();

    let mut font_definitions = FontDefinitions::default();

    font_definitions
        .font_data
        .insert("Ubuntu-Light".to_string(), FontData::from_static(font_file));

    font_definitions
        .families
        .insert(FontFamily::Monospace, vec!["Ubuntu-Light".to_string()]);
    font_definitions
        .families
        .insert(FontFamily::Proportional, vec!["Ubuntu-Light".to_string()]);

    Box::new(|cc| {
        cc.egui_ctx.set_fonts(font_definitions);

        // Selection is very bugged...
        cc.egui_ctx
            .style_mut(|s| s.interaction.selectable_labels = false);

        let texture_atlas = TextureAtlas::new(cc.egui_ctx.clone(), Color32::WHITE);

        let layout_mode = SelectedLayoutMode::FillWidthAndHeight;
        let mut editor = CosmicEdit::new(
            14.0,
            LineHeight::Relative(1.5),
            Interactivity::Enabled,
            HoverStrategy::Widget,
            layout_mode.into_layout_mode(),
            &mut font_system,
        );

        let attrs = Attrs::new()
            .family(Family::Name("Ubuntu"))
            .weight(Weight::LIGHT);
        editor.set_text(
            [
                ("This text is editable!\n\n🦀🦀🦀🦀🚀🚀🚀🚀\n\nThese emojis come from the Twitter Emoji project", attrs)
            ],
            attrs,
            Shaping::Advanced,
            &mut font_system,
        );

        let mut bottom_text = CosmicEdit::new(
            14.0,
            LineHeight::Relative(1.5),
            Interactivity::Selection,
            HoverStrategy::BoundingBox,
            PureBoundingBox::default(),
            &mut font_system,
        );

        bottom_text.set_text(
            [
                ("You can also use it as a label! Try ", attrs),
                (
                    "selecting this ",
                    attrs
                        .metrics(Metrics::new(20.0, 20.0 * 1.5))
                        .color(cosmic_text::Color::rgb(137, 207, 240)),
                ),
                ("text!", attrs),
                #[cfg(target_arch = "wasm32")]
                (" Pasting via context menu isn't supported in this WASM demo.", attrs)
            ],
            attrs,
            Shaping::Advanced,
            &mut font_system,
        );

        Ok(Box::new(DemoApp {
            font_system,
            swash_cache,
            texture_atlas,
            editor,
            bottom_text,
            #[cfg(not(target_arch = "wasm32"))]
            clipboard: Clipboard::new().expect("expected clipboard"),
            font_size: 14.0,
            rel_line_height: 1.5,
            selected_layout_mode: layout_mode,
            updated_max_texture_side: false,
            show_texture_atlas: false,
        }))
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    eframe::run_native(
        "demo",
        NativeOptions {
            follow_system_theme: false,
            ..Default::default()
        },
        app_creator(),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    _ = eframe::WebLogger::init(LevelFilter::Debug);

    wasm_bindgen_futures::spawn_local(async move {
        let start_result = eframe::WebRunner::new()
            .start(
                "the_canvas_id",
                WebOptions {
                    follow_system_theme: false,
                    ..Default::default()
                },
                app_creator(),
            )
            .await;

        // Remove the loading text and spinner:
        let loading_text = eframe::web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("loading_text"));
        if let Some(loading_text) = loading_text {
            match start_result {
                Ok(_) => {
                    loading_text.remove();
                }
                Err(e) => {
                    loading_text.set_inner_html(
                        "<p> The app has crashed. See the developer console for details. </p>",
                    );
                    panic!("Failed to start eframe: {e:?}");
                }
            }
        }
    })
}
