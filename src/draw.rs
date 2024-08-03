use crate::atlas::TextureAtlas;
use crate::util::cursor_rect;
use cosmic_text::{Buffer, Cursor, FontSystem, LayoutGlyph, LayoutRun, SwashCache};
use egui::{Painter, Pos2, Rangef, Rect};
use std::hash::BuildHasher;

enum PeekedLine<H> {
    Peeked(Option<H>),
    End,
}

/// Draws a buffer.
///
/// `min_pos`, `clip_rect`, `hover_pos` is expected to be in **logical pixels**
///
/// `measure_hover_box_width` is expected to be in *physical pixels*
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
    let pixels_per_point = painter.ctx().pixels_per_point();

    let visible_y_range = clip_rect.y_range();

    let line_y_range =
        |run: &LayoutRun| {
            Rangef::new(
                min_pos.y + (run.line_top / pixels_per_point),
                min_pos.y + ((run.line_top + run.line_height) / pixels_per_point)
            )
        };

    let selection_end_cursor_rect = selection_end
        .and_then(|x| cursor_rect(buf, x))
        // convert from physical pixels to logical points
        .map(|x| x / pixels_per_point)
        .map(|rect| rect.translate(min_pos.to_vec2()));

    let mut peeked_highlighted_line: PeekedLine<H> = PeekedLine::Peeked(None);

    let mut hovered_already = false;

    let mut layout_run_iter = buf
        .layout_runs()
        .skip_while(move |x| !visible_y_range.intersects(line_y_range(x)))
        .take_while(move |x| {
            let line_y_range = line_y_range(x);
            visible_y_range.intersects(line_y_range)
        })
        .peekable();

    while let Some(run) = layout_run_iter.next() {
        let line_y_range = line_y_range(&run);

        if let Some(hover_pos) = hover_pos {
            if !hovered_already {
                let hover_box_width = measure_hover_box_width(run.glyphs);
                if let Some(hover_box_width) = hover_box_width {
                    let bounding_box = Rect::from_x_y_ranges(
                        min_pos.x..=min_pos.x + (hover_box_width / pixels_per_point),
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

/// `rect` is expected to be in **logical pixels**
pub fn draw_run<S: BuildHasher + Default>(
    layout_run: &LayoutRun,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    atlas: &mut TextureAtlas<S>,
    painter: &mut Painter,
    rect: Rect,
) {
    let pixels_per_point = painter.ctx().pixels_per_point();

    layout_run.glyphs.iter().for_each(|glyph| {
        // convert from logical pixels to physical pixels
        let physical_glyph = glyph.physical((rect.min * pixels_per_point).into(), 1.0);
        if let Some(glyph_img) = atlas.alloc(physical_glyph.cache_key, font_system, swash_cache) {
            glyph_img.paint(glyph, physical_glyph, layout_run, painter)
        }
    })
}
