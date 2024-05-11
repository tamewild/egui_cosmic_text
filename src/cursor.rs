use cosmic_text::{Affinity, Buffer, Cursor, LayoutLine, LayoutRun};

// There's an issue here where if the first line is only spaces, it can get to a certain point where the cursor is invalid.
// I believe this happens in cosmic-edit too so it might be a cosmic-text bug.
// The editor gets into a state where the cursor goes past all the glyphs. Presumably this is where the buffer should've wrapped.
pub fn cursor_pos(buf: &Buffer, cursor: Cursor) -> Option<(f32, f32)> {
    let layout_lines_before = buf
        .lines
        .iter()
        .enumerate()
        .take(cursor.line)
        .filter_map(|(_, line)| line.layout_opt().as_ref().map(|x| x.len()))
        .sum::<usize>();

    let line_height = buf.metrics().line_height;

    let height_before_cursor_line = layout_lines_before as f32 * line_height;

    if cursor.index == 0 {
        return Some((0.0, height_before_cursor_line));
    }

    let line = buf.lines.get(cursor.line)?;
    let layout_lines_vec = line.layout_opt().as_ref()?;

    let layout_lines = layout_lines_vec.iter().enumerate();

    let mut last_line = None::<(&LayoutLine, f32)>;

    // https://github.com/iced-rs/iced/blob/dd249a1d11c68b8fee1828d58bae158946ee2ebd/graphics/src/text/editor.rs#L176
    for (i, layout_line) in layout_lines {
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

        let line_top = height_before_cursor_line + (i as f32 * line_height);

        if is_cursor_before_start {
            return last_line.map(|(line, line_top)| (line.w, line_top));
        } else if is_cursor_before_end {
            let offset = layout_line
                .glyphs
                .iter()
                .take_while(|glyph| cursor.index > glyph.start)
                .map(|glyph| glyph.w)
                .sum();
            return Some((offset, line_top));
        }

        last_line = Some((layout_line, line_top));
    }

    let last_glyph = layout_lines_vec.last().and_then(|x| x.glyphs.last());
    if let Some(last_glyph) = last_glyph {
        let last_glyph_index = last_glyph.end;
        if last_glyph_index == cursor.index {
            let last_layout_line_i = layout_lines_vec.len() - 1;
            let height_offset = last_layout_line_i as f32 * line_height;
            return Some((
                last_glyph.x + last_glyph.w,
                height_before_cursor_line + height_offset,
            ));
        }
    }

    None
}

#[derive(Debug)]
pub struct LineSelection {
    x_left: f32,
    x_width: f32,
    line_top: f32,
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
                if (run.glyphs.is_empty() || end.index == 0)
                    && buffer_line_range.contains(&run.line_i)
                {
                    (0.0, 0.0).into()
                } else {
                    None
                }
            })
            .map(|(x_left, x_width)| {
                let end_of_line = run.glyphs.last().map(|x| x.x + x.w).unwrap_or_default();
                LineSelection {
                    x_left,
                    x_width,
                    line_top: run.line_top,
                    end_of_line_included: x_left + x_width == end_of_line,
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
}
