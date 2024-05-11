use crate::atlas::TextureAtlas;
use crate::util::cursor_rect;
use cosmic_text::{Buffer, Cursor, FontSystem, LayoutGlyph, LayoutRun, SwashCache};
use egui::{Painter, Pos2, Rangef, Rect};
use std::hash::BuildHasher;

enum PeekedLine<H> {
    Peeked(Option<H>),
    End,
}

pub fn draw_buf<H>(
    buf: &Buffer,
    min_pos: Pos2,
    clip_rect: Rect,
    hover_pos: Option<Pos2>,
    selection_end: Option<Cursor>,
    painter: &mut Painter,
    measure_hover_box_width: impl Fn(&[LayoutGlyph]) -> Option<f32>,
    mut on_hover: impl FnMut(),
    highlight_single_line: impl Fn(&LayoutRun) -> Option<H>,
    mut draw_line_highlight: impl FnMut(H, bool, &mut Painter),
    mut draw_run: impl FnMut(&LayoutRun, &mut Painter),
) {
    let line_height = buf.metrics().line_height;
    let visible_y_range = clip_rect.y_range();
    let line_y_range =
        move |line_top: f32| Rangef::new(min_pos.y + line_top, min_pos.y + line_top + line_height);
    let selection_end_cursor_rect = selection_end
        .and_then(|x| cursor_rect(buf, x))
        .map(|rect| rect.translate(min_pos.to_vec2()));
    let mut peeked_highlighted_line: PeekedLine<H> = PeekedLine::Peeked(None);
    let mut hovered_already = false;
    let mut layout_run_iter = buf
        .layout_runs()
        .skip_while(move |x| !visible_y_range.intersects(line_y_range(x.line_top)))
        .take_while(move |x| {
            let line_y_range = line_y_range(x.line_top);
            visible_y_range.intersects(line_y_range)
        })
        .peekable();
    while let Some(run) = layout_run_iter.next() {
        let line_y_range = line_y_range(run.line_top);

        if let Some(hover_pos) = hover_pos {
            if !hovered_already {
                let hover_box_width = measure_hover_box_width(run.glyphs);
                if let Some(hover_box_width) = hover_box_width {
                    let bounding_box = Rect::from_x_y_ranges(
                        min_pos.x..=min_pos.x + hover_box_width,
                        line_y_range,
                    );
                    let hover = bounding_box.contains(hover_pos);
                    if hover {
                        on_hover();
                    }
                    hovered_already |= hover;
                }
            }
        }

        if let PeekedLine::Peeked(ref mut h) = peeked_highlighted_line {
            let highlighted = h.take().or_else(|| highlight_single_line(&run));

            if let Some(highlighted) = highlighted {
                *h = layout_run_iter.peek().and_then(&highlight_single_line);

                let is_end_visible = selection_end_cursor_rect
                    .is_some_and(|rect| visible_y_range.intersects(rect.y_range()));

                let last = if is_end_visible { h.is_none() } else { false };

                if last {
                    peeked_highlighted_line = PeekedLine::End;
                }

                draw_line_highlight(highlighted, last, painter);
            }
        }

        draw_run(&run, painter);
    }
}

pub fn draw_run<S: BuildHasher + Default>(
    layout_run: &LayoutRun,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    atlas: &mut TextureAtlas<S>,
    painter: &mut Painter,
    rect: Rect,
) {
    layout_run.glyphs.iter().for_each(|glyph| {
        let physical_glyph = glyph.physical(rect.min.into(), 1.0);
        if let Some(glyph_img) = atlas.alloc(physical_glyph.cache_key, font_system, swash_cache) {
            glyph_img.paint(glyph, physical_glyph, layout_run, painter)
        }
    })
}
