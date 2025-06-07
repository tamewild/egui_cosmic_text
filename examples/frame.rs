use cosmic_text::{Attrs, FontSystem, Shaping, SwashCache};
use eframe::NativeOptions;
use egui::{CentralPanel, Color32, Frame, Margin, ScrollArea};
use egui_cosmic_text::atlas::TextureAtlas;
use egui_cosmic_text::widget::{
    CosmicEdit, FillWidth, HoverStrategy, Interactivity, LineHeight, NoContextMenu,
};
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;

fn main() -> eframe::Result<()> {
    let mut font_system = FontSystem::new();
    let mut swash_cache = SwashCache::new();

    let mut atlas = None::<TextureAtlas<BuildHasherDefault<FxHasher>>>;

    let mut editor = CosmicEdit::new(
        14.0,
        LineHeight::Relative(1.5),
        Interactivity::Enabled,
        HoverStrategy::Widget,
        FillWidth::default(),
        &mut font_system,
    );
    editor.set_text([], Attrs::new(), Shaping::Advanced, &mut font_system);

    eframe::run_simple_native("", NativeOptions::default(), move |ctx, _| {
        let atlas = atlas.get_or_insert_with(|| TextureAtlas::new(ctx.clone(), Color32::WHITE));

        CentralPanel::default().show(ctx, |ui| {
            ui.label("Label");

            Frame::canvas(ui.style())
                .inner_margin(Margin::same(5))
                .show(ui, |ui| {
                    ScrollArea::vertical().show(ui, |ui| {
                        ui.centered_and_justified(|ui| {
                            editor.ui(ui, &mut font_system, &mut swash_cache, atlas, NoContextMenu)
                        });
                    });
                });
        });
    })
}
