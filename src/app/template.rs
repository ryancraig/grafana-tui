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

//! Dashboard templates: the imported layout before repeats are expanded and
//! variables are interpolated into titles.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use super::state::{GridUnit, PanelState};
use super::variables::{format_prometheus_values, substitute_variables};
use crate::dashboard::{
    DashboardAutoGrid, DashboardLayout, DashboardLayoutItem, DashboardRow, DashboardTab,
    DashboardTabs, Repeat, RepeatDirection, Repeats, RowId, TabGroupId,
};

/// Grafana's default number of horizontal repeat copies per row.
const DEFAULT_MAX_PER_ROW: u16 = 4;
const GRID_COLUMNS: i32 = 24;

/// Variable values bound by enclosing repeats, outermost first.
pub(crate) type Scope = Vec<(String, String)>;

/// The dashboard's variables, as repeats and titles see them.
pub(crate) struct Variables<'a> {
    /// Values formatted for queries.
    pub(crate) formatted: &'a HashMap<String, String>,
    /// Raw selected values.
    pub(crate) values: &'a HashMap<String, Vec<String>>,
}

impl Variables<'_> {
    /// Interpolates `$name` references in a title. A repeat's own value wins;
    /// other variables show their selected values joined like Grafana's text.
    pub(crate) fn title(&self, title: &str, scope: &Scope) -> String {
        substitute_variables(title, |name| {
            if let Some((_, value)) = scope.iter().rev().find(|(variable, _)| variable == name) {
                return Some(Cow::Owned(value.clone()));
            }
            match self.values.get(name) {
                Some(values) if !values.is_empty() => Some(Cow::Owned(values.join(" + "))),
                _ => self.formatted.get(name).map(|value| Cow::Owned(value.clone())),
            }
        })
    }

    fn repeat_values(&self, name: &str) -> &[String] {
        self.values.get(name).map_or(&[], Vec::as_slice)
    }
}

/// Query variables for a panel: the dashboard's, with each value bound by an
/// enclosing repeat formatted as that variable's only selection.
pub(crate) fn scoped_vars<'a>(
    vars: &'a HashMap<String, String>,
    scope: Option<&Scope>,
    regex: &HashSet<String>,
) -> Cow<'a, HashMap<String, String>> {
    let Some(scope) = scope.filter(|scope| !scope.is_empty()) else {
        return Cow::Borrowed(vars);
    };
    let mut vars = vars.clone();
    for (name, value) in scope {
        let formatted = format_prometheus_values(
            std::slice::from_ref(value),
            regex.contains(name.as_str()),
        );
        vars.insert(name.clone(), formatted);
    }
    Cow::Owned(vars)
}

/// The imported dashboard layout with its repeat settings and panel titles,
/// from which the displayed layout is rebuilt whenever variables change.
#[derive(Debug, Clone)]
pub(crate) struct DashboardTemplate {
    layout: DashboardLayout,
    repeats: Repeats,
    panels: Vec<PanelTemplate>,
    clones: CloneIds,
}

#[derive(Debug, Clone)]
struct PanelTemplate {
    title: String,
    grid: Option<GridUnit>,
}

/// Ids handed out to repeat copies. A copy keeps its id, and with it its panel
/// state and row or tab state, for as long as its scope stays selected.
#[derive(Debug, Clone, Default)]
struct CloneIds {
    panels: HashMap<(usize, Scope), usize>,
    rows: HashMap<(RowId, Scope), RowId>,
    tab_groups: HashMap<(TabGroupId, Scope), TabGroupId>,
    next_panel: usize,
    next_row: usize,
    next_tab_group: usize,
}

impl CloneIds {
    fn panel(&mut self, source: usize, scope: &Scope) -> usize {
        *self
            .panels
            .entry((source, scope.clone()))
            .or_insert_with(|| next(&mut self.next_panel))
    }

    fn row(&mut self, source: RowId, scope: &Scope) -> RowId {
        *self
            .rows
            .entry((source, scope.clone()))
            .or_insert_with(|| RowId::new(next(&mut self.next_row)))
    }

    fn tab_group(&mut self, source: TabGroupId, scope: &Scope) -> TabGroupId {
        *self
            .tab_groups
            .entry((source, scope.clone()))
            .or_insert_with(|| TabGroupId::new(next(&mut self.next_tab_group)))
    }
}

fn next(counter: &mut usize) -> usize {
    let value = *counter;
    *counter += 1;
    value
}

/// A panel of a materialized layout.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PanelInstance {
    /// Position in `AppState::panels`; repeat copies get indices past the imported panels.
    pub(crate) index: usize,
    /// The imported panel this instance shows.
    pub(crate) source: usize,
    pub(crate) title: String,
    pub(crate) grid: Option<GridUnit>,
    pub(crate) scope: Scope,
}

/// A layout with repeats expanded, and the panels it shows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Materialized {
    pub(crate) layout: DashboardLayout,
    pub(crate) panels: Vec<PanelInstance>,
}

impl DashboardTemplate {
    pub(crate) fn new(layout: DashboardLayout, repeats: Repeats, panels: &[PanelState]) -> Self {
        let (max_row, max_tab_group) = max_ids(&layout.items);
        Self {
            layout,
            repeats,
            panels: panels
                .iter()
                .map(|panel| PanelTemplate {
                    title: panel.title.clone(),
                    grid: panel.grid,
                })
                .collect(),
            clones: CloneIds {
                next_panel: panels.len(),
                next_row: max_row.map_or(0, |id| id + 1),
                next_tab_group: max_tab_group.map_or(0, |id| id + 1),
                ..CloneIds::default()
            },
        }
    }

    /// Expands repeats for the current variable values.
    ///
    /// The first copy of each repeated item keeps the item's own id, so a
    /// dashboard without repeats materializes to its imported layout.
    pub(crate) fn materialize(&mut self, variables: &Variables) -> Materialized {
        let mut builder = Builder {
            repeats: &self.repeats,
            templates: &self.panels,
            clones: &mut self.clones,
            variables,
            panels: Vec::new(),
        };
        let items = builder.items(&self.layout.items, &Scope::new(), true);
        Materialized {
            layout: DashboardLayout::new(items),
            panels: builder.panels,
        }
    }
}

fn max_ids(items: &[DashboardLayoutItem]) -> (Option<usize>, Option<usize>) {
    let mut rows = None;
    let mut tab_groups = None;
    for item in items {
        let (child_rows, child_tab_groups) = match item {
            DashboardLayoutItem::Row(row) => {
                rows = rows.max(Some(row.id.value()));
                max_ids(&row.children)
            }
            DashboardLayoutItem::Tabs(group) => {
                tab_groups = tab_groups.max(Some(group.id.value()));
                group
                    .tabs
                    .iter()
                    .map(|tab| max_ids(&tab.children))
                    .fold((None, None), |(a, b), (c, d)| (a.max(c), b.max(d)))
            }
            DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => (None, None),
        };
        rows = rows.max(child_rows);
        tab_groups = tab_groups.max(child_tab_groups);
    }
    (rows, tab_groups)
}

/// One copy of a repeated item: its scope, and whether it is the item itself.
struct RepeatCopy {
    scope: Scope,
    primary: bool,
}

struct Builder<'a, 'v> {
    repeats: &'a Repeats,
    templates: &'a [PanelTemplate],
    clones: &'a mut CloneIds,
    variables: &'a Variables<'v>,
    panels: Vec<PanelInstance>,
}

impl Builder<'_, '_> {
    fn items(
        &mut self,
        items: &[DashboardLayoutItem],
        scope: &Scope,
        primary: bool,
    ) -> Vec<DashboardLayoutItem> {
        let mut output = Vec::new();
        let mut panels = Vec::new();
        for item in items {
            if let DashboardLayoutItem::Panel(index) = item {
                panels.push(*index);
                continue;
            }
            self.panel_group(&std::mem::take(&mut panels), scope, primary, &mut output);
            match item {
                DashboardLayoutItem::Panel(_) => unreachable!(),
                DashboardLayoutItem::Row(row) => self.row(row, scope, primary, &mut output),
                DashboardLayoutItem::Tabs(group) => {
                    output.push(self.tabs(group, scope, primary));
                }
                DashboardLayoutItem::AutoGrid(grid) => {
                    let panels = grid
                        .panels
                        .iter()
                        .flat_map(|&source| self.panel_copies(source, scope, primary))
                        .map(|(index, _)| index)
                        .collect();
                    output.push(DashboardLayoutItem::AutoGrid(DashboardAutoGrid {
                        panels,
                        ..grid.clone()
                    }));
                }
            }
        }
        self.panel_group(&panels, scope, primary, &mut output);
        output
    }

    /// Expands a run of sibling grid panels. A repeated panel grows to hold its
    /// copies, and later siblings move down by the height it adds, as Grafana's
    /// grid does when a repeat expands.
    fn panel_group(
        &mut self,
        sources: &[usize],
        scope: &Scope,
        primary: bool,
        output: &mut Vec<DashboardLayoutItem>,
    ) {
        let mut placed = Vec::new();
        let mut shifts = Vec::new();
        for &source in sources {
            let copies = self.panel_copies(source, scope, primary);
            if let Some(grid) = self.templates[source].grid {
                let bottom = copies
                    .iter()
                    .filter_map(|(index, _)| self.instance(*index)?.grid)
                    .map(|grid| grid.y + grid.h)
                    .max()
                    .unwrap_or(grid.y + grid.h);
                let added = bottom - (grid.y + grid.h);
                if added > 0 {
                    shifts.push((grid.y, added));
                }
            }
            placed.extend(copies.into_iter().map(|(index, _)| (source, index)));
        }

        for (source, index) in placed {
            let shift: i32 = match self.templates[source].grid {
                Some(grid) => shifts
                    .iter()
                    .filter(|(from, _)| grid.y > *from)
                    .map(|(_, added)| added)
                    .sum(),
                None => 0,
            };
            if shift > 0
                && let Some(grid) = self.instance_mut(index).and_then(|panel| panel.grid.as_mut())
            {
                grid.y += shift;
            }
            output.push(DashboardLayoutItem::Panel(index));
        }
    }

    /// Records the panel instances a template panel expands to, returning their
    /// indices with whether each is the primary copy.
    fn panel_copies(&mut self, source: usize, scope: &Scope, primary: bool) -> Vec<(usize, bool)> {
        let repeat = self.repeats.panels.get(&source);
        let copies = self.copies(repeat, scope, primary);
        let count = copies.len();
        let template = &self.templates[source];
        let (title, grid) = (template.title.clone(), template.grid);
        copies
            .into_iter()
            .enumerate()
            .map(|(position, copy)| {
                let index = if copy.primary {
                    source
                } else {
                    self.clones.panel(source, &copy.scope)
                };
                let grid = match (grid, repeat) {
                    (Some(grid), Some(repeat)) => Some(repeat_grid(grid, repeat, position, count)),
                    (grid, _) => grid,
                };
                self.panels.push(PanelInstance {
                    index,
                    source,
                    title: self.variables.title(&title, &copy.scope),
                    grid,
                    scope: copy.scope,
                });
                (index, copy.primary)
            })
            .collect()
    }

    fn row(
        &mut self,
        row: &DashboardRow,
        scope: &Scope,
        primary: bool,
        output: &mut Vec<DashboardLayoutItem>,
    ) {
        for copy in self.copies(self.repeats.rows.get(&row.id), scope, primary) {
            let id = if copy.primary {
                row.id
            } else {
                self.clones.row(row.id, &copy.scope)
            };
            let children = self.items(&row.children, &copy.scope, copy.primary);
            output.push(DashboardLayoutItem::Row(DashboardRow::new(
                id,
                self.variables.title(&row.title, &copy.scope),
                row.collapsed,
                row.hidden_header,
                children,
            )));
        }
    }

    fn tabs(&mut self, group: &DashboardTabs, scope: &Scope, primary: bool) -> DashboardLayoutItem {
        let id = if primary {
            group.id
        } else {
            self.clones.tab_group(group.id, scope)
        };
        let mut tabs = Vec::new();
        for (position, tab) in group.tabs.iter().enumerate() {
            let repeat = self.repeats.tabs.get(&(group.id, position));
            for copy in self.copies(repeat, scope, primary) {
                tabs.push(DashboardTab {
                    title: self.variables.title(&tab.title, &copy.scope),
                    children: self.items(&tab.children, &copy.scope, copy.primary),
                });
            }
        }
        let mut materialized = DashboardTabs::new(id, tabs);
        if let Some(active) = group.active {
            materialized.active = Some(active.min(materialized.tabs.len().saturating_sub(1)))
                .filter(|_| !materialized.tabs.is_empty());
        }
        DashboardLayoutItem::Tabs(materialized)
    }

    /// The copies of an item repeated over its variable's selected values. An
    /// item without a repeat, or whose variable has no values, appears once.
    fn copies(&self, repeat: Option<&Repeat>, scope: &Scope, primary: bool) -> Vec<RepeatCopy> {
        let values = repeat.map_or(&[][..], |repeat| {
            self.variables.repeat_values(&repeat.variable)
        });
        let Some(repeat) = repeat.filter(|_| !values.is_empty()) else {
            return vec![RepeatCopy {
                scope: scope.clone(),
                primary,
            }];
        };
        values
            .iter()
            .enumerate()
            .map(|(position, value)| {
                let mut scope = scope.clone();
                scope.push((repeat.variable.clone(), value.clone()));
                RepeatCopy {
                    scope,
                    primary: primary && position == 0,
                }
            })
            .collect()
    }

    fn instance(&self, index: usize) -> Option<&PanelInstance> {
        self.panels.iter().rev().find(|panel| panel.index == index)
    }

    fn instance_mut(&mut self, index: usize) -> Option<&mut PanelInstance> {
        self.panels.iter_mut().rev().find(|panel| panel.index == index)
    }
}

/// Grid position of copy `position` of `count` of a repeated panel.
///
/// Matches Grafana's `DashboardGridItem`: horizontal repeats span the full grid
/// width in rows of up to `maxPerRow` equal columns, and vertical repeats stack
/// at the panel's own width.
fn repeat_grid(grid: GridUnit, repeat: &Repeat, position: usize, count: usize) -> GridUnit {
    let position = i32::try_from(position).unwrap_or(i32::MAX);
    match repeat.direction {
        RepeatDirection::Vertical => GridUnit {
            y: grid.y + position * grid.h,
            ..grid
        },
        RepeatDirection::Horizontal => {
            let per_row = i32::from(repeat.max_per_row.unwrap_or(DEFAULT_MAX_PER_ROW).max(1))
                .min(i32::try_from(count).unwrap_or(i32::MAX))
                .max(1);
            let column = position % per_row;
            let row = position / per_row;
            // Spread the grid columns as evenly as whole units allow.
            let base = GRID_COLUMNS / per_row;
            let wider = GRID_COLUMNS % per_row;
            GridUnit {
                x: column * base + column.min(wider),
                y: grid.y + row * grid.h,
                w: base + i32::from(column < wider),
                h: grid.h,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(x: i32, y: i32, w: i32, h: i32) -> Option<GridUnit> {
        Some(GridUnit { x, y, w, h })
    }

    fn panels(specs: &[(&str, Option<GridUnit>)]) -> Vec<PanelState> {
        specs
            .iter()
            .map(|(title, grid)| {
                let mut panel = crate::app::default_queries(vec!["up".to_string()]).remove(0);
                panel.title = title.to_string();
                panel.grid = *grid;
                panel
            })
            .collect()
    }

    struct Vars {
        formatted: HashMap<String, String>,
        values: HashMap<String, Vec<String>>,
    }

    impl Vars {
        fn new(selections: &[(&str, &[&str])]) -> Self {
            let values: HashMap<String, Vec<String>> = selections
                .iter()
                .map(|(name, values)| {
                    (
                        name.to_string(),
                        values.iter().map(|value| value.to_string()).collect(),
                    )
                })
                .collect();
            let formatted = values
                .iter()
                .map(|(name, values)| (name.clone(), format_prometheus_values(values, true)))
                .collect();
            Self { formatted, values }
        }

        fn get(&self) -> Variables<'_> {
            Variables {
                formatted: &self.formatted,
                values: &self.values,
            }
        }
    }

    fn repeat(variable: &str, direction: RepeatDirection, max_per_row: Option<u16>) -> Repeat {
        Repeat {
            variable: variable.to_string(),
            direction,
            max_per_row,
        }
    }

    fn grids(materialized: &Materialized) -> Vec<(usize, Option<GridUnit>)> {
        materialized
            .panels
            .iter()
            .map(|panel| (panel.index, panel.grid))
            .collect()
    }

    /// `v2_grafana13_repeats.json` was authored through Grafana 13.2.3's V2 API: a
    /// horizontal grid repeat over an `All` selection, a repeated row holding an
    /// auto grid, and repeated tabs.
    #[test]
    fn grafana13_repeats_fixture_expands_like_grafana() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/grafana/v2_grafana13_repeats.json");
        let import = crate::grafana::load_grafana_dashboard(&path).unwrap();
        assert!(import.diagnostics.is_empty(), "{:?}", import.diagnostics);
        let panels: Vec<PanelState> = import
            .queries
            .iter()
            .map(|query| {
                let mut panel = crate::app::default_queries(vec!["up".to_string()]).remove(0);
                panel.title = query.title.clone();
                panel.grid = query.grid.map(|grid| GridUnit {
                    x: grid.x,
                    y: grid.y,
                    w: grid.w,
                    h: grid.h,
                });
                panel
            })
            .collect();
        let mut template =
            DashboardTemplate::new(import.layout.clone(), import.repeats.clone(), &panels);

        let materialized = template.materialize(&Variables {
            formatted: &import.vars,
            values: &import.var_values,
        });

        let titles: Vec<_> = materialized
            .layout
            .items
            .iter()
            .map(|item| match item {
                DashboardLayoutItem::Row(row) => row.title.as_str(),
                item => panic!("expected rows, got {item:?}"),
            })
            .collect();
        assert_eq!(titles, ["Handlers", "Quantile 0.5", "Quantile 0.99", "By tab"]);

        let handlers: Vec<_> = materialized
            .panels
            .iter()
            .filter(|panel| panel.source <= 1)
            .map(|panel| (panel.title.as_str(), panel.grid))
            .collect();
        // Three handlers at two per row, then `Up` moved below the extra row.
        assert_eq!(
            handlers,
            [
                ("Requests /api/v1/query", grid(0, 0, 12, 6)),
                ("Requests /api/v1/query_range", grid(12, 0, 12, 6)),
                ("Requests /metrics", grid(0, 6, 12, 6)),
                ("Up", grid(0, 12, 24, 4)),
            ]
        );

        let DashboardLayoutItem::Row(by_tab) = &materialized.layout.items[3] else {
            unreachable!()
        };
        let [DashboardLayoutItem::Tabs(tabs)] = by_tab.children.as_slice() else {
            panic!("expected tabs, got {:?}", by_tab.children);
        };
        let tab_titles: Vec<_> = tabs.tabs.iter().map(|tab| tab.title.as_str()).collect();
        assert_eq!(tab_titles, ["/api/v1/query", "/api/v1/query_range", "/metrics"]);
    }

    #[test]
    fn layouts_without_repeats_materialize_unchanged_with_interpolated_titles() {
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(0),
            "Jobs: $job",
            true,
            false,
            vec![DashboardLayoutItem::Panel(0)],
        ))]);
        let mut template = DashboardTemplate::new(
            layout.clone(),
            Repeats::default(),
            &panels(&[("Up for ${job} / $job_name", grid(0, 0, 12, 4))]),
        );
        let vars = Vars::new(&[("job", &["api", "web"])]);

        let materialized = template.materialize(&vars.get());

        let DashboardLayoutItem::Row(row) = &materialized.layout.items[0] else {
            panic!("expected a row");
        };
        assert_eq!(row.title, "Jobs: api + web");
        assert!(row.collapsed);
        assert_eq!(row.children, [DashboardLayoutItem::Panel(0)]);
        // `$job_name` is a different, undefined variable and stays as written.
        assert_eq!(materialized.panels[0].title, "Up for api + web / $job_name");
        assert!(materialized.panels[0].scope.is_empty());
    }

    #[test]
    fn horizontal_repeats_fill_the_grid_width_and_push_later_panels_down() {
        let mut repeats = Repeats::default();
        repeats
            .panels
            .insert(0, repeat("dc", RepeatDirection::Horizontal, None));
        let mut template = DashboardTemplate::new(
            DashboardLayout::flat(2),
            repeats,
            &panels(&[("CPU $dc", grid(6, 0, 12, 4)), ("Below", grid(0, 4, 24, 4))]),
        );
        let vars = Vars::new(&[("dc", &["a", "b", "c", "d", "e"])]);

        let materialized = template.materialize(&vars.get());

        // Five copies wrap after Grafana's default four per row, each 6 units wide.
        assert_eq!(
            grids(&materialized),
            [
                (0, grid(0, 0, 6, 4)),
                (2, grid(6, 0, 6, 4)),
                (3, grid(12, 0, 6, 4)),
                (4, grid(18, 0, 6, 4)),
                (5, grid(0, 4, 6, 4)),
                (1, grid(0, 8, 24, 4)),
            ]
        );
        let titles: Vec<_> = materialized.panels.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(titles, ["CPU a", "CPU b", "CPU c", "CPU d", "CPU e", "Below"]);
        assert_eq!(materialized.panels[4].scope, [("dc".to_string(), "e".to_string())]);
    }

    #[test]
    fn horizontal_repeats_spread_uneven_columns_and_honor_max_per_row() {
        let mut repeats = Repeats::default();
        repeats
            .panels
            .insert(0, repeat("dc", RepeatDirection::Horizontal, Some(5)));
        let mut template = DashboardTemplate::new(
            DashboardLayout::flat(1),
            repeats,
            &panels(&[("CPU", grid(0, 2, 8, 3))]),
        );
        let vars = Vars::new(&[("dc", &["a", "b", "c", "d", "e"])]);

        let materialized = template.materialize(&vars.get());

        let widths: Vec<_> = materialized
            .panels
            .iter()
            .map(|panel| panel.grid.unwrap())
            .map(|grid| (grid.x, grid.w, grid.y))
            .collect();
        assert_eq!(widths, [(0, 5, 2), (5, 5, 2), (10, 5, 2), (15, 5, 2), (20, 4, 2)]);
    }

    #[test]
    fn vertical_repeats_stack_and_move_only_panels_starting_below_the_source() {
        let mut repeats = Repeats::default();
        repeats
            .panels
            .insert(0, repeat("dc", RepeatDirection::Vertical, None));
        let mut template = DashboardTemplate::new(
            DashboardLayout::flat(3),
            repeats,
            &panels(&[
                ("CPU", grid(0, 0, 12, 4)),
                ("Beside", grid(12, 0, 12, 4)),
                ("Below", grid(0, 4, 12, 4)),
            ]),
        );
        let vars = Vars::new(&[("dc", &["a", "b", "c"])]);

        let materialized = template.materialize(&vars.get());

        assert_eq!(
            grids(&materialized),
            [
                (0, grid(0, 0, 12, 4)),
                (3, grid(0, 4, 12, 4)),
                (4, grid(0, 8, 12, 4)),
                (1, grid(12, 0, 12, 4)),
                (2, grid(0, 12, 12, 4)),
            ]
        );
    }

    #[test]
    fn repeated_rows_copy_their_subtree_with_new_ids_and_scopes() {
        let mut repeats = Repeats::default();
        repeats.rows.insert(RowId::new(4), Repeat::new("dc"));
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(4),
            "Region $dc",
            false,
            false,
            vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
                TabGroupId::new(2),
                vec![DashboardTab {
                    title: "Tab".to_string(),
                    children: vec![DashboardLayoutItem::Panel(0)],
                }],
            ))],
        ))]);
        let mut template =
            DashboardTemplate::new(layout, repeats, &panels(&[("CPU $dc", None)]));
        let vars = Vars::new(&[("dc", &["eu", "us"])]);

        let materialized = template.materialize(&vars.get());

        let rows: Vec<_> = materialized
            .layout
            .items
            .iter()
            .map(|item| match item {
                DashboardLayoutItem::Row(row) => match row.children.as_slice() {
                    [DashboardLayoutItem::Tabs(tabs)] => (row.id, row.title.clone(), tabs.id),
                    children => panic!("unexpected row children {children:?}"),
                },
                item => panic!("unexpected item {item:?}"),
            })
            .collect();
        assert_eq!(
            rows,
            [
                (RowId::new(4), "Region eu".to_string(), TabGroupId::new(2)),
                (RowId::new(5), "Region us".to_string(), TabGroupId::new(3)),
            ]
        );
        let panels: Vec<_> = materialized
            .panels
            .iter()
            .map(|panel| (panel.index, panel.source, panel.title.as_str()))
            .collect();
        assert_eq!(panels, [(0, 0, "CPU eu"), (1, 0, "CPU us")]);
    }

    #[test]
    fn repeated_tabs_and_auto_grid_items_expand_in_place() {
        let mut repeats = Repeats::default();
        repeats
            .tabs
            .insert((TabGroupId::new(0), 0), Repeat::new("dc"));
        repeats.panels.insert(0, Repeat::new("host"));
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
            TabGroupId::new(0),
            vec![DashboardTab {
                title: "$dc".to_string(),
                children: vec![DashboardLayoutItem::AutoGrid(DashboardAutoGrid {
                    panels: vec![0],
                    max_columns: 3,
                    min_column_width: 56,
                    row_height: 9,
                })],
            }],
        ))]);
        let mut template = DashboardTemplate::new(layout, repeats, &panels(&[("$host", None)]));
        let vars = Vars::new(&[("dc", &["eu", "us"]), ("host", &["a", "b"])]);

        let materialized = template.materialize(&vars.get());

        let DashboardLayoutItem::Tabs(group) = &materialized.layout.items[0] else {
            panic!("expected tabs");
        };
        let tabs: Vec<_> = group
            .tabs
            .iter()
            .map(|tab| match tab.children.as_slice() {
                [DashboardLayoutItem::AutoGrid(grid)] => (tab.title.as_str(), grid.panels.clone()),
                children => panic!("unexpected tab children {children:?}"),
            })
            .collect();
        assert_eq!(tabs, [("eu", vec![0, 1]), ("us", vec![2, 3])]);
        let scopes: Vec<_> = materialized
            .panels
            .iter()
            .map(|panel| panel.scope.clone())
            .collect();
        let scope = |dc: &str, host: &str| {
            vec![
                ("dc".to_string(), dc.to_string()),
                ("host".to_string(), host.to_string()),
            ]
        };
        assert_eq!(
            scopes,
            [scope("eu", "a"), scope("eu", "b"), scope("us", "a"), scope("us", "b")]
        );
    }

    #[test]
    fn copies_keep_their_ids_across_materializations() {
        let mut repeats = Repeats::default();
        repeats.panels.insert(0, Repeat::new("dc"));
        let mut template =
            DashboardTemplate::new(DashboardLayout::flat(1), repeats, &panels(&[("$dc", None)]));

        let first = template.materialize(&Vars::new(&[("dc", &["a", "b", "c"])]).get());
        let again = template.materialize(&Vars::new(&[("dc", &["a", "b", "c"])]).get());
        let fewer = template.materialize(&Vars::new(&[("dc", &["a", "c"])]).get());

        assert_eq!(first, again);
        let indices = |materialized: &Materialized| -> Vec<usize> {
            materialized.panels.iter().map(|panel| panel.index).collect()
        };
        assert_eq!(indices(&first), [0, 1, 2]);
        assert_eq!(indices(&fewer), [0, 2]);
    }

    #[test]
    fn repeats_without_selected_values_show_the_item_once() {
        let mut repeats = Repeats::default();
        repeats.panels.insert(0, Repeat::new("dc"));
        repeats.rows.insert(RowId::new(0), Repeat::new("dc"));
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(0),
            "Row",
            false,
            false,
            vec![DashboardLayoutItem::Panel(0)],
        ))]);
        let mut template = DashboardTemplate::new(layout.clone(), repeats, &panels(&[("$dc", None)]));

        let materialized = template.materialize(&Vars::new(&[]).get());

        assert_eq!(materialized.layout, layout);
        assert_eq!(materialized.panels[0].title, "$dc");
    }

    #[test]
    fn scoped_vars_format_repeat_values_like_a_single_selection() {
        let vars = HashMap::from([
            ("dc".to_string(), "(eu|us)".to_string()),
            ("job".to_string(), "api".to_string()),
        ]);
        let regex = HashSet::from(["dc".to_string()]);
        let scope = vec![("dc".to_string(), "eu.west".to_string())];

        let scoped = scoped_vars(&vars, Some(&scope), &regex);

        assert_eq!(scoped.get("dc").map(String::as_str), Some("eu\\\\.west"));
        assert_eq!(scoped.get("job").map(String::as_str), Some("api"));
        assert!(matches!(scoped_vars(&vars, None, &regex), Cow::Borrowed(_)));
    }
}
