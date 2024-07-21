use cosmic_text::{Affinity, Buffer, Cursor, LayoutRun};
use egui::{pos2, Rect, vec2};

// There's an issue here where if the first line is only spaces, it can get to a certain point where the cursor is invalid.
// I believe this happens in cosmic-edit too so it might be a cosmic-text bug.
// The editor gets into a state where the cursor goes past all the glyphs. Presumably this is where the buffer should've wrapped.
pub fn cursor_pos(buf: &Buffer, cursor: Cursor) -> Option<Rect> {
    buf.layout_runs().find_map(|run| {
        run.cursor_position(&cursor).map(|(x, y)| {
            Rect::from_min_size(
                pos2(x, y),
                vec2(1.0, run.line_height)
            )
        })
    })
}

fn end_cursor(run: &LayoutRun) -> Option<Cursor> {
    match run.rtl {
        true => {
            // |..
            run.glyphs.first().map(|glyph| {
                Cursor::new_with_affinity(run.line_i, glyph.start, Affinity::After)
            })
        }
        false => {
            // ..|
            run.glyphs.last().map(|glyph| {
                Cursor::new_with_affinity(run.line_i, glyph.end, Affinity::Before)
            })
        }
    }
}

#[derive(Debug)]
pub struct LineSelection {
    x_left: f32,
    x_width: f32,
    line_top: f32,
    line_height: f32,
    end_of_line_included: bool,
}

impl LineSelection {
    pub fn new(run: &LayoutRun, (start, end): (Cursor, Cursor)) -> Option<Self> {
        run.highlight(start, end)
            .or_else(|| {
                // Highlight function is based on glyphs, so it won't return anything even if
                // it's within the selection.
                // Affinity messes up with 0 indexes sometimes
                let buffer_line_range = start.line..=end.line;
                if run.glyphs.is_empty() && buffer_line_range.contains(&run.line_i)
                {
                    (0.0, 0.0).into()
                } else {
                    None
                }
            })
            .map(|(x_left, x_width)| {
                let end_of_line_included = match end_cursor(run) {
                    None => true,
                    Some(end_cursor) => end_cursor <= end
                };
                LineSelection {
                    x_left,
                    x_width,
                    line_top: run.line_top,
                    line_height: run.line_height,
                    end_of_line_included,
                }
            })
    }

    pub fn end_of_line_included(&self) -> bool {
        self.end_of_line_included
    }

    pub fn x_left_and_width(&self) -> (f32, f32) {
        (self.x_left, self.x_width)
    }

    pub fn line_top(&self) -> f32 {
        self.line_top
    }

    pub fn line_height(&self) -> f32 {
        self.line_height
    }
}
