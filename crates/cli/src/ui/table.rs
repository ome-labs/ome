//! One table style: no borders, dim headers, wrapping to the terminal. `--plain` prints the rows tab-separated
//! without headers, for `grep` and `awk`.

use comfy_table::{Attribute, Cell, Color, ContentArrangement, presets};

use crate::context::{Ctx, Mode};

/// A cell's meaning, which decides its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Plain,
    Ok,
    Warn,
    Bad,
    Name,
    Dim,
    Bold,
}

/// A table of rows.
pub(crate) struct Table {
    headers: Vec<&'static str>,
    rows: Vec<Vec<(String, Tone)>>,
}

impl Table {
    pub(crate) fn new(headers: &[&'static str]) -> Self {
        Self {
            headers: headers.to_vec(),
            rows: Vec::new(),
        }
    }

    pub(crate) fn row(&mut self, cells: Vec<(String, Tone)>) {
        self.rows.push(cells);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The table as text for this mode.
    pub(crate) fn render(&self, mode: Mode, color: bool) -> String {
        if mode == Mode::Plain {
            return self
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|(text, _)| text.replace(['\t', '\n'], " "))
                        .collect::<Vec<_>>()
                        .join("\t")
                })
                .collect::<Vec<_>>()
                .join("\n");
        }
        let mut table = comfy_table::Table::new();
        table
            .load_style(presets::NOTHING)
            .set_content_arrangement(ContentArrangement::Dynamic);
        if color {
            table.enforce_styling();
        } else {
            table.force_no_tty();
        }
        table.set_header(
            self.headers
                .iter()
                .map(|h| Cell::new(h.to_uppercase()).add_attribute(Attribute::Dim)),
        );
        for row in &self.rows {
            table.add_row(row.iter().map(|(text, tone)| styled(text, *tone)));
        }
        // comfy-table pads each line on the left by one space; drop it, so tables line up with other output.
        table
            .lines()
            .map(|line| {
                line.strip_prefix(' ')
                    .unwrap_or(&line)
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(crate) fn print(&self, ctx: &Ctx) {
        ctx.out(&self.render(ctx.mode, ctx.color()));
    }
}

fn styled(text: &str, tone: Tone) -> Cell {
    let cell = Cell::new(text);
    match tone {
        Tone::Plain => cell,
        Tone::Ok => cell.fg(Color::Green),
        Tone::Warn => cell.fg(Color::Yellow),
        Tone::Bad => cell.fg(Color::Red),
        Tone::Name => cell.fg(Color::Cyan),
        Tone::Dim => cell.add_attribute(Attribute::Dim),
        Tone::Bold => cell.add_attribute(Attribute::Bold),
    }
}

/// A plain cell.
pub(crate) fn cell(text: impl Into<String>) -> (String, Tone) {
    (text.into(), Tone::Plain)
}

/// A cell with a tone.
pub(crate) fn toned(text: impl Into<String>, tone: Tone) -> (String, Tone) {
    (text.into(), tone)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Table {
        let mut table = Table::new(&["id", "status"]);
        table.row(vec![cell("4f2ac91e"), toned("ready", Tone::Ok)]);
        table.row(vec![cell("9b10\tx"), toned("executed", Tone::Dim)]);
        table
    }

    #[test]
    fn plain_rows_are_tab_separated_without_headers() {
        assert_eq!(
            table().render(Mode::Plain, false),
            "4f2ac91e\tready\n9b10 x\texecuted"
        );
    }

    #[test]
    fn human_tables_have_headers_and_no_colour_when_off() {
        insta::assert_snapshot!(table().render(Mode::Human, false));
    }
}
