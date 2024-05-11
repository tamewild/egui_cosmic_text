use arboard::Clipboard;
use cosmic_text::{Attrs, FontSystem, Shaping, SwashCache};
use eframe::NativeOptions;
use egui::util::History;
use egui::{CentralPanel, Color32, ScrollArea, Slider};
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;

use egui_cosmic_text::atlas::TextureAtlas;
use egui_cosmic_text::widget::{
    CosmicEdit, DefaultContextMenu, FillWidthAndHeight, HoverStrategy, Interactivity, LineHeight,
    ShrinkToFit,
};

fn main() -> eframe::Result<()> {
    let mut font_system = FontSystem::new();
    let mut swash_cache = SwashCache::new();

    let mut atlas = None::<TextureAtlas<BuildHasherDefault<FxHasher>>>;

    let mut egui_text_edit = String::new();

    let mut cosmic_edit = CosmicEdit::new(
        14.0,
        LineHeight::Relative(1.5),
        Interactivity::Enabled,
        HoverStrategy::Widget,
        FillWidthAndHeight::default(),
        &mut font_system,
    );
    cosmic_edit.set_text(
        [(
            include_str!("../misc/Decently sized lorem ipsum.txt"),
            Attrs::new(),
        )],
        Attrs::new(),
        Shaping::Advanced,
        &mut font_system,
    );

    let mut cosmic_label = CosmicEdit::new(
        14.0,
        LineHeight::Relative(1.5),
        Interactivity::Selection,
        HoverStrategy::BoundingBox,
        ShrinkToFit::default(),
        &mut font_system,
    );
    cosmic_label.set_text(
        [(
            "This is a cosmic label! 👋👋👋 Try selecting this text! 🦀🦀🦀",
            Attrs::new(),
        )],
        Attrs::new(),
        Shaping::Advanced,
        &mut font_system,
    );

    let mut clipboard = Clipboard::new().ok();

    let mut frame_times = History::<()>::new(0..300, 1.0);

    let mut font_size = 14.0;

    eframe::run_simple_native("", NativeOptions::default(), move |ctx, _| {
        frame_times.add(ctx.input(|i| i.time), ());

        let atlas = atlas.get_or_insert_with(|| TextureAtlas::new(ctx.clone(), Color32::WHITE));

        CentralPanel::default().show(ctx, |ui| {
            ui.label(format!(
                "FPS: {}",
                1.0 / frame_times.mean_time_interval().unwrap_or_default()
            ));
            ui.add(Slider::new(&mut font_size, 10.0..=200.0).text("Font size"));
            ui.label("This is a native egui label 👋👋👋");
            ui.text_edit_singleline(&mut egui_text_edit);
            ui.separator();
            cosmic_label.ui(
                ui,
                &mut font_system,
                &mut swash_cache,
                atlas,
                DefaultContextMenu {
                    read_clipboard_text: || None,
                },
            );
            ui.separator();
            ScrollArea::vertical().show(ui, |ui| {
                cosmic_edit.set_font_size(font_size, LineHeight::Relative(1.5), &mut font_system);
                cosmic_edit.ui(
                    ui,
                    &mut font_system,
                    &mut swash_cache,
                    atlas,
                    DefaultContextMenu {
                        read_clipboard_text: || clipboard.as_mut().and_then(|x| x.get_text().ok()),
                    },
                );
            });
        });

        atlas.trim();
        ctx.request_repaint();
    })
}
