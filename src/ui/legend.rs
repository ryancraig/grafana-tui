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

//! Graph legend layout, shared by the terminal and exports.

/// How a renderer measures legend text, in its own units: terminal cells, or
/// pixels in an export.
pub(crate) struct LegendMetrics<'a> {
    /// Width of a swatch and the space after it.
    pub(crate) swatch: f64,
    /// Space between entries in a row.
    pub(crate) gap: f64,
    /// Width of a label.
    pub(crate) width: &'a dyn Fn(&str) -> f64,
    /// A label shortened to fit a width.
    pub(crate) shorten: &'a dyn Fn(&str, f64) -> String,
}

/// A graph legend laid out in rows.
pub(crate) struct LegendLayout {
    pub(crate) entries: Vec<LegendEntry>,
    /// Where "+N more" starts on the last row, and the text, when some
    /// entries did not fit.
    pub(crate) more: Option<(f64, String)>,
    pub(crate) rows: usize,
}

/// One legend label, with its series index and its place in the legend.
pub(crate) struct LegendEntry {
    pub(crate) index: usize,
    pub(crate) label: String,
    pub(crate) x: f64,
    pub(crate) row: usize,
}

/// Lays legend entries out in rows of `width`, shortening any label wider
/// than a row. Past `max_rows`, the last row ends with "+N more" in place of
/// the entries that do not fit.
pub(crate) fn layout_legend(
    items: Vec<(usize, String)>,
    width: f64,
    max_rows: usize,
    metrics: &LegendMetrics<'_>,
) -> LegendLayout {
    let entry_width = |label: &str| metrics.swatch + (metrics.width)(label);
    let mut entries: Vec<LegendEntry> = Vec::new();
    let (mut x, mut row) = (0.0, 0);
    for (index, label) in items {
        let label = (metrics.shorten)(&label, width - metrics.swatch);
        let label_width = entry_width(&label);
        if x > 0.0 && x + label_width > width {
            x = 0.0;
            row += 1;
        }
        entries.push(LegendEntry {
            index,
            label,
            x,
            row,
        });
        x += label_width + metrics.gap;
    }
    let rows = entries.last().map_or(0, |entry| entry.row + 1);
    if rows <= max_rows {
        return LegendLayout {
            entries,
            more: None,
            rows,
        };
    }

    let total = entries.len();
    let last_row = max_rows.max(1) - 1;
    entries.retain(|entry| entry.row <= last_row);
    loop {
        let label = format!("+{} more", total - entries.len());
        let x = match entries.last() {
            Some(entry) if entry.row == last_row => {
                entry.x + entry_width(&entry.label) + metrics.gap
            }
            _ => 0.0,
        };
        if x == 0.0 || x + (metrics.width)(&label) <= width {
            return LegendLayout {
                entries,
                more: Some((x, label)),
                rows: last_row + 1,
            };
        }
        entries.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One unit per character, a swatch of 2 and a gap of 2, as in a terminal.
    fn cells() -> LegendMetrics<'static> {
        LegendMetrics {
            swatch: 2.0,
            gap: 2.0,
            width: &|label| label.chars().count() as f64,
            shorten: &|label, width| {
                if label.chars().count() as f64 <= width {
                    label.to_string()
                } else {
                    let keep = (width as usize).saturating_sub(1);
                    label.chars().take(keep).chain(['…']).collect()
                }
            },
        }
    }

    fn items(labels: &[&str]) -> Vec<(usize, String)> {
        labels
            .iter()
            .enumerate()
            .map(|(index, label)| (index, label.to_string()))
            .collect()
    }

    #[test]
    fn entries_wrap_into_rows() {
        // Each entry is 2 + 8 wide, and 2 apart: two fit in 22.
        let legend = layout_legend(items(&["client-1"; 3]), 22.0, 3, &cells());
        assert_eq!(legend.rows, 2);
        let places = legend
            .entries
            .iter()
            .map(|entry| (entry.x, entry.row))
            .collect::<Vec<_>>();
        assert_eq!(places, [(0.0, 0), (12.0, 0), (0.0, 1)]);
        assert!(legend.more.is_none());
    }

    #[test]
    fn labels_wider_than_a_row_are_shortened() {
        let legend = layout_legend(items(&["nexus-uar-docker-token-svc"]), 12.0, 3, &cells());
        assert_eq!(legend.entries[0].label, "nexus-uar…");
    }

    #[test]
    fn entries_past_the_last_row_become_more() {
        let legend = layout_legend(items(&["client-1"; 10]), 22.0, 2, &cells());
        assert_eq!(legend.rows, 2);
        assert_eq!(legend.entries.len(), 3);
        // "+7 more" is 7 wide: it fits after the one entry on the last row.
        assert_eq!(legend.more, Some((12.0, "+7 more".to_string())));
    }

    #[test]
    fn more_replaces_entries_until_it_fits() {
        // The last row is full, so its second entry gives way to "+9 more".
        let legend = layout_legend(items(&["client-1"; 10]), 22.0, 1, &cells());
        assert_eq!(legend.entries.len(), 1);
        assert_eq!(legend.more, Some((12.0, "+9 more".to_string())));
    }
}
