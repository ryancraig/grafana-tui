use crate::annotations::visible_range;
use crate::app::AppState;
use crate::theme::{Theme, builtin_family};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};

const HINT: &str = "↑/↓ preview  Enter keep  Esc revert";
const SWATCH_WIDTH: usize = 10;

enum PickerRow<'a> {
    Family(&'a str),
    Theme(usize, &'a Theme),
}

fn family(theme: &Theme) -> &'static str {
    builtin_family(&theme.name).unwrap_or("Custom")
}

/// Themes interleaved with a heading wherever the family changes.
fn picker_rows(themes: &[Theme]) -> Vec<PickerRow<'_>> {
    let mut rows = Vec::new();
    let mut current = None;
    for (index, theme) in themes.iter().enumerate() {
        let family = family(theme);
        if current != Some(family) {
            rows.push(PickerRow::Family(family));
            current = Some(family);
        }
        rows.push(PickerRow::Theme(index, theme));
    }
    rows
}

fn picker_area(size: Size, themes: &[Theme]) -> Rect {
    let name_width = themes
        .iter()
        .map(|theme| theme.name.chars().count())
        .max()
        .unwrap_or_default();
    // Borders, the selection marker, a gap, and the swatch.
    let content_width = (name_width + 6 + SWATCH_WIDTH).max(HINT.chars().count());
    let width = u16::try_from(content_width + 2)
        .unwrap_or(u16::MAX)
        .min(size.width);
    let rows = picker_rows(themes).len() + 1;
    let height = u16::try_from(rows + 2).unwrap_or(u16::MAX).min(size.height);
    Rect::new(
        (size.width - width) / 2,
        (size.height - height) / 2,
        width,
        height,
    )
}

fn list_height(area: Rect) -> usize {
    // Borders and the hint row.
    usize::from(area.height.saturating_sub(3))
}

/// Rows a page of the picker moves.
pub(crate) fn theme_picker_page_size(size: Size, app: &AppState) -> usize {
    list_height(picker_area(size, &app.themes)).max(1)
}

pub(crate) fn render_theme_picker(frame: &mut Frame, app: &AppState) {
    let Some(picker) = app.theme_picker.as_ref() else {
        return;
    };
    let frame_area = frame.area();
    let area = picker_area(Size::new(frame_area.width, frame_area.height), &app.themes);
    let theme = &app.theme;
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .title(" Theme ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_focused))
            .style(Style::default().fg(theme.text).bg(theme.surface)),
        area,
    );
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let [list_area, hint_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);

    let rows = picker_rows(&app.themes);
    let selected_row = rows
        .iter()
        .position(|row| matches!(row, PickerRow::Theme(index, _) if *index == picker.selected))
        .unwrap_or_default();
    let visible = visible_range(rows.len(), selected_row, list_height(area));
    let name_width = usize::from(list_area.width).saturating_sub(SWATCH_WIDTH + 3);
    let lines: Vec<Line> = rows[visible]
        .iter()
        .map(|row| match row {
            PickerRow::Family(family) => Line::styled(
                *family,
                Style::default()
                    .fg(theme.title)
                    .add_modifier(Modifier::BOLD),
            ),
            PickerRow::Theme(index, candidate) => {
                let selected = *index == picker.selected;
                let style = if selected {
                    Style::default()
                        .fg(theme.selection_fg)
                        .bg(theme.selection_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text)
                };
                let marker = if selected { "▶ " } else { "  " };
                let original = if candidate.name == picker.original.name {
                    "•"
                } else {
                    " "
                };
                let mut spans = vec![Span::styled(
                    format!("{marker}{:<name_width$}{original} ", candidate.name),
                    style,
                )];
                spans.extend(swatch(candidate));
                Line::from(spans)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), list_area);
    frame.render_widget(
        Paragraph::new(HINT).style(Style::default().fg(theme.text_muted)),
        hint_area,
    );
}

/// The theme's background followed by its leading series colors.
fn swatch(theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled("██", Style::default().fg(theme.background))];
    spans.extend(
        theme
            .palette
            .iter()
            .take(SWATCH_WIDTH - 2)
            .map(|color| Span::styled("█", Style::default().fg(*color))),
    );
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::ExportOptions;
    use crate::prom::PromClient;
    use ratatui::{Terminal, backend::TestBackend};

    fn test_app() -> AppState {
        AppState::new(
            PromClient::new("http://localhost:9090".to_string()),
            std::time::Duration::from_secs(300),
            std::time::Duration::from_secs(5),
            std::time::Duration::from_secs(1),
            "Test".to_string(),
            vec![],
            0,
            Theme::default(),
            "dashed-line".to_string(),
            ExportOptions::default(),
        )
    }

    fn rendered(app: &AppState, width: u16, height: u16) -> (Terminal<TestBackend>, String) {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render_theme_picker(frame, app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text = (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        (terminal, text)
    }

    #[test]
    fn groups_themes_by_family_and_marks_the_selection() {
        let mut app = test_app();
        app.open_theme_picker();
        app.move_theme_picker(4);

        let (terminal, text) = rendered(&app, 80, 40);

        for heading in ["Tokyo Night", "Catppuccin", "Gruvbox", "Other"] {
            assert!(text.contains(heading), "missing {heading}:\n{text}");
        }
        let selected = text
            .lines()
            .find(|line| line.contains("▶ "))
            .unwrap_or_else(|| panic!("no selection:\n{text}"));
        assert!(selected.contains("catppuccin-mocha"), "{selected}");
        assert!(text.contains("tokyo-night"));
        assert!(text.contains(HINT));
        // The picker is drawn in the previewed theme.
        let buffer = terminal.backend().buffer();
        let surface_cells = buffer
            .content()
            .iter()
            .filter(|cell| cell.bg == app.theme.surface)
            .count();
        assert!(surface_cells > 0);
    }

    #[test]
    fn custom_themes_get_their_own_group() {
        let mut app = test_app();
        app.themes = crate::theme::catalog(&[Theme {
            name: "mine".to_string(),
            ..Theme::default()
        }]);
        app.open_theme_picker();
        app.move_theme_picker(isize::MAX);

        let (_, text) = rendered(&app, 80, 40);

        assert!(text.contains("Custom"), "{text}");
        let selected = text.lines().find(|line| line.contains("▶ ")).unwrap();
        assert!(selected.contains("mine"), "{selected}");
    }

    #[test]
    fn scrolls_to_keep_the_selection_visible_in_a_short_terminal() {
        let mut app = test_app();
        app.open_theme_picker();
        app.move_theme_picker(isize::MAX);

        let (_, text) = rendered(&app, 60, 12);

        assert!(text.contains("▶ terminal"), "{text}");
        assert!(!text.contains("tokyo-night-storm"), "{text}");
    }

    #[test]
    fn tiny_terminals_do_not_panic() {
        let mut app = test_app();
        app.open_theme_picker();
        for (width, height) in [(0, 0), (1, 1), (8, 3), (30, 4)] {
            rendered(&app, width, height);
            assert!(theme_picker_page_size(Size::new(width, height), &app) >= 1);
        }
    }
}
