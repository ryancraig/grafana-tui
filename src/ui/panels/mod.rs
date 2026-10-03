/*
 * Copyright 2026 Federico D'Ambrosio
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

mod bar_gauge;
mod gauge;
mod graph;
mod heatmap;
mod stat;
mod table;

use super::tabs::truncate_title;
use crate::app::{AppState, PanelState, PanelType};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use bar_gauge::render_bar_gauge;
use gauge::render_gauge;
pub(crate) use graph::calculate_y_bounds;
use graph::render_graph_panel;
use heatmap::render_heatmap;
use stat::render_stat;
use table::render_table;

/// How serious a panel's data notice is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoticeLevel {
    Error,
    Warning,
}

/// A marker shown after the title of a panel that still has data to show but
/// whose latest fetch failed or returned warnings. The footer shows the
/// details for the selected panel.
pub(crate) fn data_notice(p: &PanelState) -> Option<(&'static str, NoticeLevel)> {
    if p.last_error.is_some() {
        let text = if p.notices.stale {
            "⚠ stale: queries failed"
        } else {
            "⚠ query failed"
        };
        Some((text, NoticeLevel::Error))
    } else if !p.notices.warnings.is_empty() {
        Some(("⚠ warning", NoticeLevel::Warning))
    } else {
        None
    }
}

/// Title columns kept before a notice shrinks to its icon.
const MIN_TITLE_COLUMNS: usize = 6;

/// The notice icon alone, for panels too narrow for the whole notice.
const NOTICE_ICON: &str = "⚠";

/// Fits a panel title and its notice in `columns`, with a space between
/// them. The title is cut short first, then the notice shrinks to its icon,
/// so the notice stays in view in narrow panels.
pub(crate) fn fit_panel_title(
    title: &str,
    notice: Option<&'static str>,
    columns: usize,
) -> (String, Option<&'static str>) {
    let to_u16 = |columns: usize| u16::try_from(columns).unwrap_or(u16::MAX);
    if let Some(notice) = notice {
        let title_width = Span::raw(title).width();
        for badge in [notice, NOTICE_ICON] {
            let room = columns.saturating_sub(Span::raw(badge).width() + 1);
            if room >= title_width.min(MIN_TITLE_COLUMNS) {
                return (truncate_title(title, to_u16(room)), Some(badge));
            }
        }
    }
    (truncate_title(title, to_u16(columns)), None)
}

/// Renders a single panel.
///
/// This function handles:
/// - Drawing the panel border and title.
/// - Rendering the chart with data series.
/// - Drawing the legend (if space permits).
/// - Handling inspection mode (cursor line and values).
/// - Displaying error messages if the panel has an error.
pub(crate) fn render_panel(
    frame: &mut Frame,
    area: Rect,
    panel_index: usize,
    p: &PanelState,
    app: &AppState,
    is_selected: bool,
    cursor_x: Option<f64>,
) -> Option<Vec<crate::annotations::AnnotationEvent>> {
    let theme = &app.theme;
    let border_style = if is_selected {
        Style::default().fg(theme.border_focused)
    } else {
        Style::default().fg(theme.border)
    };

    if let Some(err) = &p.last_error
        && p.series.is_empty()
    {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(
                format!("{} — ERROR", p.title),
                Style::default().fg(theme.error),
            ));
        let para = Paragraph::new(err.clone())
            .block(block)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(theme.text));
        frame.render_widget(para, area);
        return None;
    }

    if !p.has_loaded() && !p.exprs.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(
                p.title.clone(),
                Style::default().fg(theme.title),
            ));
        let para = Paragraph::new("Loading…")
            .block(block)
            .style(Style::default().fg(theme.text));
        frame.render_widget(para, area);
        return None;
    }

    // Render the outer block (Panel container)
    let notice = data_notice(p);
    let (title_text, badge) = fit_panel_title(
        &p.title,
        notice.map(|(text, _)| text),
        usize::from(area.width.saturating_sub(2)),
    );
    let mut title = vec![Span::styled(title_text, Style::default().fg(theme.title))];
    if let (Some(badge), Some((_, level))) = (badge, notice) {
        let color = match level {
            NoticeLevel::Error => theme.error,
            NoticeLevel::Warning => theme.warning,
        };
        title.push(Span::styled(
            format!(" {badge}"),
            Style::default().fg(color),
        ));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Line::from(title));
    frame.render_widget(block.clone(), area);

    let inner_area = block.inner(area);

    match p.panel_type {
        PanelType::Graph => render_graph_panel(
            frame,
            inner_area,
            panel_index,
            p,
            app,
            cursor_x,
            is_selected,
        ),
        PanelType::Unknown => {
            render_graph_panel(frame, inner_area, panel_index, p, app, cursor_x, false);
            None
        }
        PanelType::Gauge => {
            render_gauge(frame, inner_area, p, app);
            None
        }
        PanelType::BarGauge => {
            render_bar_gauge(frame, inner_area, p, app);
            None
        }
        PanelType::Table => {
            render_table(frame, inner_area, p, app);
            None
        }
        PanelType::Stat => {
            render_stat(frame, inner_area, p, app);
            None
        }
        PanelType::Heatmap => {
            render_heatmap(frame, inner_area, p, app);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fit_panel_title;

    #[test]
    fn titles_shorten_before_the_notice_does() {
        let title = "Upstream connect failures";
        let notice = Some("⚠ warning");

        assert_eq!(
            fit_panel_title(title, notice, 40),
            (title.to_string(), notice)
        );
        // 20 columns leave 10 for the title beside " ⚠ warning".
        assert_eq!(
            fit_panel_title(title, notice, 20),
            ("Upstream …".to_string(), notice)
        );
        // Too narrow for 6 title columns: the notice shrinks to its icon.
        assert_eq!(
            fit_panel_title(title, notice, 12),
            ("Upstream …".to_string(), Some("⚠"))
        );
        // Short titles keep the whole notice for as long as they fit.
        assert_eq!(
            fit_panel_title("CPU", notice, 14),
            ("CPU".to_string(), notice)
        );
        assert_eq!(
            fit_panel_title(title, None, 9),
            ("Upstream…".to_string(), None)
        );
        assert_eq!(fit_panel_title(title, notice, 0), (String::new(), None));
    }
}
