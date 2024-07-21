use cosmic_text::{Buffer, Cursor, LayoutLine};
use egui::{pos2, vec2, Rect};

use crate::cursor;
use crate::cursor::LineSelection;

pub fn layout_lines_iter(buf: &Buffer) -> impl Iterator<Item = &LayoutLine> {
    buf.lines
        .iter()
        .filter_map(|x| x.layout_opt().as_ref())
        .flatten()
}

/// Measures the maximum height of the runs that have been laid out.
pub fn measure_height(buf: &Buffer) -> f32 {
    layout_lines_iter(buf)
        .map(|x| x.line_height_opt.unwrap_or(buf.metrics().line_height))
        .sum()
}

/// Measures the maximum width and maximum height of the runs that have been laid out.
pub fn measure_width_and_height(buf: &Buffer) -> (f32, f32) {
    let base_line_height = buf.metrics().line_height;
    layout_lines_iter(buf)
        .fold((0.0, 0.0), |(width, height), line| {
            (line.w.max(width), height + line.line_height_opt.unwrap_or(base_line_height))
        })
}

/// Attempts to retrieve the cursor's rect from inside the buffer.
/// This has to be translated to the widget's rect and is relative to the buffer, starting from `0.0, 0.0`
pub fn cursor_rect(buf: &Buffer, cursor: Cursor) -> Option<Rect> {
    cursor::cursor_pos(buf, cursor)
}

pub fn extra_width(line_height: f32) -> f32 {
    // https://github.com/emilk/egui/blob/b8048572e8cc47ef9410b3516456da2a320fcdd2/crates/egui/src/text_selection/visuals.rs#L36
    line_height / 2.0
}

pub fn selection_rect(line_selection: LineSelection, line_height: f32, last: bool) -> Rect {
    let extra_width = extra_width(line_height);
    let (x_left, mut x_width) = line_selection.x_left_and_width();
    if !last && line_selection.end_of_line_included() {
        x_width += extra_width;
    }
    Rect::from_min_size(
        pos2(x_left, line_selection.line_top()),
        vec2(x_width, line_height),
    )
}
