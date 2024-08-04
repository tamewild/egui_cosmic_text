use cosmic_text::{Affinity, Buffer, Cursor, LayoutLine, LayoutRun};
use egui::{pos2, vec2, Rect};

// There's an issue here where if the first line is only spaces, it can get to a certain point where the cursor is invalid.
// I believe this happens in cosmic-edit too so it might be a cosmic-text bug.
// The editor gets into a state where the cursor goes past all the glyphs. Presumably this is where the buffer should've wrapped.
/// **In physical pixels.**
pub fn cursor_pos(buf: &Buffer, cursor: Cursor) -> Option<Rect> {
    let base_line_height = buf.metrics().line_height;

    let height_before_cursor_line = buf
        .lines
        .iter()
        .enumerate()
        .take(cursor.line)
        .filter_map(|(_, line)| line.layout_opt().as_ref())
        .flatten()
        .map(|x| x.line_height_opt.unwrap_or(base_line_height))
        .sum();

    if cursor.index == 0 {
        let line_height = buf
            .lines
            .get(cursor.line)
            .and_then(|x| x.layout_opt().as_ref())
            .and_then(|x| x.first())
            .map(|x| x.line_height_opt.unwrap_or(base_line_height))?;

        return Some(Rect::from_min_size(
            pos2(0.0, height_before_cursor_line),
            vec2(1.0, line_height),
        ));
    }

    let line = buf.lines.get(cursor.line)?;
    let layout_lines_vec = line.layout_opt().as_ref()?;

    let mut last_line = None::<(&LayoutLine, f32)>;

    let mut line_top = height_before_cursor_line;

    // https://github.com/iced-rs/iced/blob/dd249a1d11c68b8fee1828d58bae158946ee2ebd/graphics/src/text/editor.rs#L176
    for layout_line in layout_lines_vec.iter() {
        let start = layout_line
            .glyphs
            .first()
            .map(|x| x.start)
            .unwrap_or_default();
        let end = layout_line.glyphs.last().map(|x| x.end).unwrap_or_default();

        let is_cursor_before_start = start > cursor.index;

        let is_cursor_before_end = match cursor.affinity {
            Affinity::Before => cursor.index <= end,
            Affinity::After => cursor.index < end,
        };

        if is_cursor_before_start {
            return last_line.map(|(line, line_top)| {
                Rect::from_min_size(
                    pos2(line.w, line_top),
                    vec2(1.0, line.line_height_opt.unwrap_or(base_line_height)),
                )
            });
        } else if is_cursor_before_end {
            let offset = layout_line
                .glyphs
                .iter()
                .take_while(|glyph| cursor.index > glyph.start)
                .map(|glyph| glyph.w)
                .sum();
            return Some(Rect::from_min_size(
                pos2(offset, line_top),
                vec2(1.0, layout_line.line_height_opt.unwrap_or(base_line_height)),
            ));
        }

        last_line = Some((layout_line, line_top));

        line_top += layout_line.line_height_opt.unwrap_or(base_line_height);
    }

    let last_glyph = layout_lines_vec.last().and_then(|x| x.glyphs.last());
    if let Some(last_glyph) = last_glyph {
        let last_glyph_index = last_glyph.end;
        if last_glyph_index == cursor.index {
            let (line, line_top) = last_line?;
            return Some(Rect::from_min_size(
                pos2(last_glyph.x + last_glyph.w, line_top),
                vec2(1.0, line.line_height_opt.unwrap_or(base_line_height)),
            ));
        }
    }

    None
}

fn end_cursor(run: &LayoutRun) -> Option<Cursor> {
    match run.rtl {
        true => {
            // |..
            run.glyphs
                .first()
                .map(|glyph| Cursor::new_with_affinity(run.line_i, glyph.start, Affinity::After))
        }
        false => {
            // ..|
            run.glyphs
                .last()
                .map(|glyph| Cursor::new_with_affinity(run.line_i, glyph.end, Affinity::Before))
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
                if run.glyphs.is_empty() && buffer_line_range.contains(&run.line_i) {
                    (0.0, 0.0).into()
                } else {
                    None
                }
            })
            .map(|(x_left, x_width)| {
                let end_of_line_included = match end_cursor(run) {
                    None => true,
                    Some(end_cursor) => end_cursor <= end,
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

    /// In **physical pixels**
    pub fn x_left_and_width(&self) -> (f32, f32) {
        (self.x_left, self.x_width)
    }

    /// In **physical pixels**
    pub fn line_top(&self) -> f32 {
        self.line_top
    }

    /// In **physical pixels**
    pub fn line_height(&self) -> f32 {
        self.line_height
    }
}
