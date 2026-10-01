use std::collections::{HashMap, HashSet};

use crate::conditions::ConditionTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct RowId(usize);

impl RowId {
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TabGroupId(usize);

impl TabGroupId {
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> usize {
        self.0
    }
}

/// Direction in which a repeated panel's copies are laid out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RepeatDirection {
    /// Side by side across the full grid width, wrapping after `max_per_row`.
    #[default]
    Horizontal,
    /// Stacked below each other at the source panel's width.
    Vertical,
}

/// Repeats an item once per selected value of a dashboard variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Repeat {
    pub(crate) variable: String,
    pub(crate) direction: RepeatDirection,
    /// Copies per row for horizontal panel repeats; Grafana defaults to 4.
    pub(crate) max_per_row: Option<u16>,
}

impl Repeat {
    #[cfg(test)]
    pub(crate) fn new(variable: impl Into<String>) -> Self {
        Self {
            variable: variable.into(),
            direction: RepeatDirection::default(),
            max_per_row: None,
        }
    }
}

/// Repeat settings of an imported layout, keyed by the ids it was imported with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Repeats {
    pub(crate) panels: HashMap<usize, Repeat>,
    pub(crate) rows: HashMap<RowId, Repeat>,
    /// Keyed by tab group and the tab's position within it.
    pub(crate) tabs: HashMap<(TabGroupId, usize), Repeat>,
}

impl Repeats {
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.panels.is_empty() && self.rows.is_empty() && self.tabs.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DashboardItemId {
    Row(RowId),
    Tabs(TabGroupId),
    Panel(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DashboardLayoutItem {
    Row(DashboardRow),
    Tabs(DashboardTabs),
    Panel(usize),
    AutoGrid(DashboardAutoGrid),
}

/// Panels flowed row-major into equal-width columns (Grafana V2 `AutoGridLayout`).
///
/// Unlike rows and tabs, an auto grid is not selectable and has no header: its
/// panels belong to the enclosing container, and only their placement differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardAutoGrid {
    pub(crate) panels: Vec<usize>,
    pub(crate) max_columns: u16,
    /// Minimum column width in terminal cells.
    pub(crate) min_column_width: u16,
    /// Row height in dashboard grid units, the unit of `GridPos::h`.
    pub(crate) row_height: u16,
}

impl DashboardAutoGrid {
    /// Number of columns for `width` cells.
    ///
    /// Mirrors Grafana's CSS `repeat(auto-fit, minmax(...))` track list with a
    /// one-cell gap: as many minimum-width columns as fit, at most `max_columns`,
    /// and never more than there are panels because `auto-fit` collapses empty
    /// tracks so the remaining panels stretch.
    pub(crate) fn column_count(&self, width: u16) -> u16 {
        let fitting = width.saturating_add(1) / self.min_column_width.saturating_add(1);
        let panels = u16::try_from(self.panels.len()).unwrap_or(u16::MAX);
        fitting.min(self.max_columns).min(panels).max(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardTab {
    pub(crate) title: String,
    pub(crate) children: Vec<DashboardLayoutItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardTabs {
    pub(crate) id: TabGroupId,
    pub(crate) tabs: Vec<DashboardTab>,
    pub(crate) active: Option<usize>,
}

impl DashboardTabs {
    pub(crate) fn new(id: TabGroupId, tabs: Vec<DashboardTab>) -> Self {
        let active = (!tabs.is_empty()).then_some(0);
        Self { id, tabs, active }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardRow {
    pub(crate) id: RowId,
    pub(crate) title: String,
    pub(crate) collapsed: bool,
    pub(crate) hidden_header: bool,
    pub(crate) children: Vec<DashboardLayoutItem>,
}

impl DashboardRow {
    pub(crate) fn new(
        id: RowId,
        title: impl Into<String>,
        collapsed: bool,
        hidden_header: bool,
        children: Vec<DashboardLayoutItem>,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            collapsed,
            hidden_header,
            children,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleDashboardItem {
    pub(crate) id: DashboardItemId,
    pub(crate) depth: usize,
}

impl VisibleDashboardItem {
    pub(crate) const fn panel(index: usize, depth: usize) -> Self {
        Self {
            id: DashboardItemId::Panel(index),
            depth,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DashboardLayout {
    pub(crate) items: Vec<DashboardLayoutItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LayoutChange {
    pub(crate) newly_visible_panels: Vec<usize>,
}

impl DashboardLayout {
    pub(crate) fn new(items: Vec<DashboardLayoutItem>) -> Self {
        Self { items }
    }

    pub(crate) fn flat(panel_count: usize) -> Self {
        Self::new((0..panel_count).map(DashboardLayoutItem::Panel).collect())
    }

    pub(crate) fn row(&self, id: RowId) -> Option<&DashboardRow> {
        find_row(&self.items, id)
    }

    pub(crate) fn tabs(&self, id: TabGroupId) -> Option<&DashboardTabs> {
        find_tabs(&self.items, id)
    }

    pub(crate) fn visible_items(&self) -> Vec<VisibleDashboardItem> {
        let mut visible = Vec::new();
        collect_visible_items(&self.items, 0, &mut visible);
        visible
    }

    pub(crate) fn visible_panel_indices(&self) -> Vec<usize> {
        self.visible_items()
            .into_iter()
            .filter_map(|item| match item.id {
                DashboardItemId::Panel(index) => Some(index),
                DashboardItemId::Row(_) | DashboardItemId::Tabs(_) => None,
            })
            .collect()
    }

    pub(crate) fn visible_panel_count(&self) -> usize {
        self.visible_panel_indices().len()
    }

    pub(crate) fn toggle_row(&mut self, id: RowId) -> Option<LayoutChange> {
        let collapsed = !self.row(id)?.collapsed;
        self.set_row_collapsed(id, collapsed)
    }

    pub(crate) fn set_row_collapsed(&mut self, id: RowId, collapsed: bool) -> Option<LayoutChange> {
        let before = self.visible_panel_indices();
        find_row_mut(&mut self.items, id)?.collapsed = collapsed;
        let after = self.visible_panel_indices();
        Some(LayoutChange {
            newly_visible_panels: after
                .into_iter()
                .filter(|panel| !before.contains(panel))
                .collect(),
        })
    }

    pub(crate) fn set_active_tab(&mut self, id: TabGroupId, index: usize) -> Option<LayoutChange> {
        let tabs = self.tabs(id)?;
        if index >= tabs.tabs.len() {
            return None;
        }
        if tabs.active == Some(index) {
            return Some(LayoutChange::default());
        }
        let before = self.visible_panel_indices();
        find_tabs_mut(&mut self.items, id)?.active = Some(index);
        let after = self.visible_panel_indices();
        Some(LayoutChange {
            newly_visible_panels: after
                .into_iter()
                .filter(|panel| !before.contains(panel))
                .collect(),
        })
    }

    pub(crate) fn first_tab_descendant(&self, id: TabGroupId) -> Option<DashboardItemId> {
        let group = self.tabs(id)?;
        let tab = group.active.and_then(|index| group.tabs.get(index))?;
        let mut visible = Vec::new();
        collect_visible_items(&tab.children, 0, &mut visible);
        visible.first().map(|item| item.id)
    }

    pub(crate) fn first_visible(&self) -> Option<DashboardItemId> {
        self.visible_items().first().map(|item| item.id)
    }

    /// Copies row collapse state and active tabs from `previous` onto the rows and
    /// tab groups with the same ids, so rebuilding a layout keeps what was open.
    pub(crate) fn restore_state(&mut self, previous: &DashboardLayout) {
        let mut rows = HashMap::new();
        let mut tabs = HashMap::new();
        collect_state(&previous.items, &mut rows, &mut tabs);
        apply_state(&mut self.items, &rows, &tabs);
    }

    /// This layout without the rows, tabs, and auto grid panels in `hidden`.
    ///
    /// Tab groups and auto grids left empty are dropped. A tab group whose
    /// active tab is hidden activates its first remaining tab.
    pub(crate) fn without(&self, hidden: &HashSet<ConditionTarget>) -> DashboardLayout {
        DashboardLayout::new(filter_items(&self.items, hidden))
    }

    /// Copies row collapse state and active tabs back from `displayed`, which
    /// is this layout `without` the `hidden` items, so the state survives
    /// rebuilding the displayed layout for a new set of hidden items.
    pub(crate) fn sync_state_from(
        &mut self,
        displayed: &DashboardLayout,
        hidden: &HashSet<ConditionTarget>,
    ) {
        let mut rows = HashMap::new();
        let mut tabs = HashMap::new();
        collect_state(&displayed.items, &mut rows, &mut tabs);
        sync_state(&mut self.items, &rows, &tabs, hidden);
    }

    pub(crate) fn nearest_visible_ancestor(&self, id: DashboardItemId) -> Option<DashboardItemId> {
        let visible = self.visible_items();
        if visible.iter().any(|item| item.id == id) {
            return Some(id);
        }

        let mut ancestors = Vec::new();
        if find_ancestors(&self.items, id, &mut ancestors) {
            for ancestor in ancestors.into_iter().rev() {
                if visible.iter().any(|item| item.id == ancestor) {
                    return Some(ancestor);
                }
            }
        }

        self.first_visible()
    }
}

fn filter_items(
    items: &[DashboardLayoutItem],
    hidden: &HashSet<ConditionTarget>,
) -> Vec<DashboardLayoutItem> {
    items
        .iter()
        .filter_map(|item| match item {
            DashboardLayoutItem::Panel(index) => (!hidden.contains(&ConditionTarget::Panel(*index)))
                .then(|| item.clone()),
            DashboardLayoutItem::Row(row) => (!hidden.contains(&ConditionTarget::Row(row.id)))
                .then(|| {
                    DashboardLayoutItem::Row(DashboardRow {
                        children: filter_items(&row.children, hidden),
                        ..row.clone()
                    })
                }),
            DashboardLayoutItem::Tabs(group) => {
                let shown: Vec<usize> = (0..group.tabs.len())
                    .filter(|&position| !hidden.contains(&ConditionTarget::Tab(group.id, position)))
                    .collect();
                if shown.is_empty() {
                    return None;
                }
                let active = group
                    .active
                    .and_then(|active| shown.iter().position(|&position| position == active))
                    .unwrap_or(0);
                Some(DashboardLayoutItem::Tabs(DashboardTabs {
                    id: group.id,
                    tabs: shown
                        .iter()
                        .map(|&position| {
                            let tab = &group.tabs[position];
                            DashboardTab {
                                title: tab.title.clone(),
                                children: filter_items(&tab.children, hidden),
                            }
                        })
                        .collect(),
                    active: Some(active),
                }))
            }
            DashboardLayoutItem::AutoGrid(grid) => {
                let panels: Vec<usize> = grid
                    .panels
                    .iter()
                    .copied()
                    .filter(|index| !hidden.contains(&ConditionTarget::Panel(*index)))
                    .collect();
                (!panels.is_empty()).then(|| {
                    DashboardLayoutItem::AutoGrid(DashboardAutoGrid {
                        panels,
                        ..grid.clone()
                    })
                })
            }
        })
        .collect()
}

/// Applies displayed row and tab state to a layout that also holds `hidden`
/// items, translating each displayed active tab back to its position among
/// all of the group's tabs.
fn sync_state(
    items: &mut [DashboardLayoutItem],
    rows: &HashMap<RowId, bool>,
    tabs: &HashMap<TabGroupId, Option<usize>>,
    hidden: &HashSet<ConditionTarget>,
) {
    for item in items {
        match item {
            DashboardLayoutItem::Row(row) => {
                if let Some(&collapsed) = rows.get(&row.id) {
                    row.collapsed = collapsed;
                }
                sync_state(&mut row.children, rows, tabs, hidden);
            }
            DashboardLayoutItem::Tabs(group) => {
                if let Some(&Some(displayed)) = tabs.get(&group.id) {
                    let id = group.id;
                    group.active = (0..group.tabs.len())
                        .filter(|&position| !hidden.contains(&ConditionTarget::Tab(id, position)))
                        .nth(displayed)
                        .or(group.active);
                }
                for tab in &mut group.tabs {
                    sync_state(&mut tab.children, rows, tabs, hidden);
                }
            }
            DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => {}
        }
    }
}

fn collect_state(
    items: &[DashboardLayoutItem],
    rows: &mut HashMap<RowId, bool>,
    tabs: &mut HashMap<TabGroupId, Option<usize>>,
) {
    for item in items {
        match item {
            DashboardLayoutItem::Row(row) => {
                rows.insert(row.id, row.collapsed);
                collect_state(&row.children, rows, tabs);
            }
            DashboardLayoutItem::Tabs(group) => {
                tabs.insert(group.id, group.active);
                for tab in &group.tabs {
                    collect_state(&tab.children, rows, tabs);
                }
            }
            DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => {}
        }
    }
}

fn apply_state(
    items: &mut [DashboardLayoutItem],
    rows: &HashMap<RowId, bool>,
    tabs: &HashMap<TabGroupId, Option<usize>>,
) {
    for item in items {
        match item {
            DashboardLayoutItem::Row(row) => {
                if let Some(&collapsed) = rows.get(&row.id) {
                    row.collapsed = collapsed;
                }
                apply_state(&mut row.children, rows, tabs);
            }
            DashboardLayoutItem::Tabs(group) => {
                if let Some(&active) = tabs.get(&group.id) {
                    group.active = active
                        .map(|index| index.min(group.tabs.len().saturating_sub(1)))
                        .filter(|_| !group.tabs.is_empty());
                }
                for tab in &mut group.tabs {
                    apply_state(&mut tab.children, rows, tabs);
                }
            }
            DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => {}
        }
    }
}

fn find_row(items: &[DashboardLayoutItem], id: RowId) -> Option<&DashboardRow> {
    for item in items {
        if let DashboardLayoutItem::Row(row) = item {
            if row.id == id {
                return Some(row);
            }
            if let Some(found) = find_row(&row.children, id) {
                return Some(found);
            }
        } else if let DashboardLayoutItem::Tabs(group) = item {
            for tab in &group.tabs {
                if let Some(found) = find_row(&tab.children, id) {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn find_row_mut(items: &mut [DashboardLayoutItem], id: RowId) -> Option<&mut DashboardRow> {
    for item in items {
        if let DashboardLayoutItem::Row(row) = item {
            if row.id == id {
                return Some(row);
            }
            if let Some(found) = find_row_mut(&mut row.children, id) {
                return Some(found);
            }
        } else if let DashboardLayoutItem::Tabs(group) = item {
            for tab in &mut group.tabs {
                if let Some(found) = find_row_mut(&mut tab.children, id) {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn find_tabs(items: &[DashboardLayoutItem], id: TabGroupId) -> Option<&DashboardTabs> {
    for item in items {
        match item {
            DashboardLayoutItem::Tabs(group) => {
                if group.id == id {
                    return Some(group);
                }
                for tab in &group.tabs {
                    if let Some(found) = find_tabs(&tab.children, id) {
                        return Some(found);
                    }
                }
            }
            DashboardLayoutItem::Row(row) => {
                if let Some(found) = find_tabs(&row.children, id) {
                    return Some(found);
                }
            }
            DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => {}
        }
    }
    None
}

fn find_tabs_mut(items: &mut [DashboardLayoutItem], id: TabGroupId) -> Option<&mut DashboardTabs> {
    for item in items {
        match item {
            DashboardLayoutItem::Tabs(group) => {
                if group.id == id {
                    return Some(group);
                }
                for tab in &mut group.tabs {
                    if let Some(found) = find_tabs_mut(&mut tab.children, id) {
                        return Some(found);
                    }
                }
            }
            DashboardLayoutItem::Row(row) => {
                if let Some(found) = find_tabs_mut(&mut row.children, id) {
                    return Some(found);
                }
            }
            DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => {}
        }
    }
    None
}

fn collect_visible_items(
    items: &[DashboardLayoutItem],
    depth: usize,
    visible: &mut Vec<VisibleDashboardItem>,
) {
    for item in items {
        match item {
            DashboardLayoutItem::Panel(index) => {
                visible.push(VisibleDashboardItem::panel(*index, depth))
            }
            DashboardLayoutItem::Row(row) if row.hidden_header => {
                collect_visible_items(&row.children, depth, visible);
            }
            DashboardLayoutItem::Row(row) => {
                visible.push(VisibleDashboardItem {
                    id: DashboardItemId::Row(row.id),
                    depth,
                });
                if !row.collapsed {
                    collect_visible_items(&row.children, depth + 1, visible);
                }
            }
            DashboardLayoutItem::Tabs(group) => {
                visible.push(VisibleDashboardItem {
                    id: DashboardItemId::Tabs(group.id),
                    depth,
                });
                if let Some(tab) = group.active.and_then(|index| group.tabs.get(index)) {
                    collect_visible_items(&tab.children, depth + 1, visible);
                }
            }
            DashboardLayoutItem::AutoGrid(grid) => visible.extend(
                grid.panels
                    .iter()
                    .map(|&index| VisibleDashboardItem::panel(index, depth)),
            ),
        }
    }
}

fn find_ancestors(
    items: &[DashboardLayoutItem],
    target: DashboardItemId,
    ancestors: &mut Vec<DashboardItemId>,
) -> bool {
    for item in items {
        match item {
            DashboardLayoutItem::Panel(index) if target == DashboardItemId::Panel(*index) => {
                return true;
            }
            DashboardLayoutItem::Row(row) => {
                if target == DashboardItemId::Row(row.id) {
                    return true;
                }
                ancestors.push(DashboardItemId::Row(row.id));
                if find_ancestors(&row.children, target, ancestors) {
                    return true;
                }
                ancestors.pop();
            }
            DashboardLayoutItem::Tabs(group) => {
                if target == DashboardItemId::Tabs(group.id) {
                    return true;
                }
                ancestors.push(DashboardItemId::Tabs(group.id));
                for tab in &group.tabs {
                    if find_ancestors(&tab.children, target, ancestors) {
                        return true;
                    }
                }
                ancestors.pop();
            }
            DashboardLayoutItem::AutoGrid(grid) => {
                if grid
                    .panels
                    .iter()
                    .any(|&index| target == DashboardItemId::Panel(index))
                {
                    return true;
                }
            }
            DashboardLayoutItem::Panel(_) => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_switch_returns_only_revealed_panels() {
        let id = TabGroupId::new(0);
        let mut layout = DashboardLayout::new(vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
            id,
            vec![
                DashboardTab {
                    title: "A".into(),
                    children: vec![DashboardLayoutItem::Panel(0)],
                },
                DashboardTab {
                    title: "B".into(),
                    children: vec![DashboardLayoutItem::Panel(1)],
                },
            ],
        ))]);
        assert_eq!(layout.visible_panel_indices(), vec![0]);
        assert_eq!(
            layout.set_active_tab(id, 1).unwrap().newly_visible_panels,
            vec![1]
        );
        assert_eq!(layout.visible_panel_indices(), vec![1]);
        assert!(
            layout
                .set_active_tab(id, 1)
                .unwrap()
                .newly_visible_panels
                .is_empty()
        );
        assert_eq!(layout.set_active_tab(id, 2), None);
        assert_eq!(
            layout.nearest_visible_ancestor(DashboardItemId::Panel(0)),
            Some(DashboardItemId::Tabs(id))
        );
    }

    #[test]
    fn nested_tab_and_row_state_survive_outer_switches() {
        let outer_id = TabGroupId::new(0);
        let inner_id = TabGroupId::new(1);
        let mut layout = DashboardLayout::new(vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
            outer_id,
            vec![
                DashboardTab {
                    title: "Nested".into(),
                    children: vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
                        inner_id,
                        vec![
                            DashboardTab {
                                title: "First".into(),
                                children: vec![DashboardLayoutItem::Row(DashboardRow::new(
                                    RowId::new(0),
                                    "Collapsed",
                                    true,
                                    false,
                                    vec![DashboardLayoutItem::Panel(0)],
                                ))],
                            },
                            DashboardTab {
                                title: "Second".into(),
                                children: vec![DashboardLayoutItem::Panel(1)],
                            },
                        ],
                    ))],
                },
                DashboardTab {
                    title: "Other".into(),
                    children: vec![DashboardLayoutItem::Panel(2)],
                },
            ],
        ))]);

        layout.set_active_tab(inner_id, 1).unwrap();
        layout.set_active_tab(outer_id, 1).unwrap();
        layout.set_active_tab(outer_id, 0).unwrap();

        assert_eq!(layout.tabs(inner_id).unwrap().active, Some(1));
        assert!(layout.row(RowId::new(0)).unwrap().collapsed);
        assert_eq!(layout.visible_panel_indices(), vec![1]);
    }

    #[test]
    fn empty_tab_groups_remain_visible_and_safe_to_enter() {
        let id = TabGroupId::new(0);
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
            id,
            vec![],
        ))]);

        assert_eq!(layout.first_visible(), Some(DashboardItemId::Tabs(id)));
        assert_eq!(layout.tabs(id).unwrap().active, None);
        assert_eq!(layout.first_tab_descendant(id), None);
        assert!(layout.visible_panel_indices().is_empty());
    }

    #[test]
    fn collapsed_parent_hides_descendants_and_preserves_nested_state() {
        let nested = DashboardRow::new(
            RowId::new(2),
            "Nested",
            true,
            false,
            vec![DashboardLayoutItem::Panel(2)],
        );
        let parent = DashboardRow::new(
            RowId::new(1),
            "Parent",
            false,
            false,
            vec![
                DashboardLayoutItem::Panel(0),
                DashboardLayoutItem::Row(nested),
                DashboardLayoutItem::Panel(1),
            ],
        );
        let mut layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(parent)]);

        assert_eq!(layout.visible_panel_indices(), vec![0, 1]);
        layout.set_row_collapsed(RowId::new(1), true).unwrap();
        assert_eq!(layout.visible_panel_indices(), Vec::<usize>::new());
        let change = layout.set_row_collapsed(RowId::new(1), false).unwrap();
        assert_eq!(change.newly_visible_panels, vec![0, 1]);
        assert!(layout.row(RowId::new(2)).unwrap().collapsed);
    }

    #[test]
    fn hidden_header_is_transparent_even_when_marked_collapsed() {
        let hidden = DashboardRow::new(
            RowId::new(1),
            "Hidden",
            true,
            true,
            vec![DashboardLayoutItem::Panel(0)],
        );
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(hidden)]);
        assert_eq!(layout.visible_panel_indices(), vec![0]);
        assert_eq!(
            layout.visible_items(),
            vec![VisibleDashboardItem::panel(0, 0)]
        );
    }

    fn auto_grid(panels: Vec<usize>) -> DashboardLayoutItem {
        DashboardLayoutItem::AutoGrid(DashboardAutoGrid {
            panels,
            max_columns: 3,
            min_column_width: 10,
            row_height: 4,
        })
    }

    fn conditional_layout() -> DashboardLayout {
        DashboardLayout::new(vec![
            DashboardLayoutItem::Row(DashboardRow::new(
                RowId::new(0),
                "Hidden row",
                false,
                false,
                vec![DashboardLayoutItem::Panel(0)],
            )),
            DashboardLayoutItem::Tabs(DashboardTabs {
                id: TabGroupId::new(0),
                tabs: vec![
                    DashboardTab {
                        title: "A".to_string(),
                        children: vec![DashboardLayoutItem::Row(DashboardRow::new(
                            RowId::new(1),
                            "In A",
                            false,
                            false,
                            vec![],
                        ))],
                    },
                    DashboardTab {
                        title: "B".to_string(),
                        children: vec![auto_grid(vec![1, 2])],
                    },
                    DashboardTab {
                        title: "C".to_string(),
                        children: vec![],
                    },
                ],
                active: Some(0),
            }),
        ])
    }

    #[test]
    fn hidden_conditional_items_are_left_out_of_the_layout() {
        let layout = conditional_layout();
        let hidden = HashSet::from([
            ConditionTarget::Row(RowId::new(0)),
            ConditionTarget::Tab(TabGroupId::new(0), 0),
            ConditionTarget::Panel(1),
        ]);

        let shown = layout.without(&hidden);

        let [DashboardLayoutItem::Tabs(group)] = shown.items.as_slice() else {
            panic!("expected only the tab group, got {:?}", shown.items);
        };
        let titles: Vec<_> = group.tabs.iter().map(|tab| tab.title.as_str()).collect();
        assert_eq!(titles, ["B", "C"]);
        // The active tab was hidden, so the first remaining tab is active.
        assert_eq!(group.active, Some(0));
        assert_eq!(group.tabs[0].children, [auto_grid(vec![2])]);
        assert_eq!(shown.visible_panel_indices(), [2]);
    }

    #[test]
    fn tab_groups_and_auto_grids_with_nothing_left_are_dropped() {
        let layout = DashboardLayout::new(vec![
            auto_grid(vec![0]),
            DashboardLayoutItem::Tabs(DashboardTabs::new(
                TabGroupId::new(0),
                vec![DashboardTab {
                    title: "Only".to_string(),
                    children: vec![],
                }],
            )),
        ]);
        let hidden = HashSet::from([
            ConditionTarget::Panel(0),
            ConditionTarget::Tab(TabGroupId::new(0), 0),
        ]);

        assert!(layout.without(&hidden).items.is_empty());
    }

    #[test]
    fn displayed_state_syncs_back_through_hidden_items() {
        let mut layout = conditional_layout();
        let hidden = HashSet::from([ConditionTarget::Tab(TabGroupId::new(0), 0)]);
        let mut displayed = layout.without(&hidden);
        // Activate "C", the second displayed tab, and collapse the first row.
        displayed.set_active_tab(TabGroupId::new(0), 1);
        displayed.set_row_collapsed(RowId::new(0), true);

        layout.sync_state_from(&displayed, &hidden);

        assert_eq!(layout.tabs(TabGroupId::new(0)).unwrap().active, Some(2));
        assert!(layout.row(RowId::new(0)).unwrap().collapsed);
        // Showing every tab again keeps "C" active.
        let shown = layout.without(&HashSet::new());
        assert_eq!(shown.tabs(TabGroupId::new(0)).unwrap().active, Some(2));
    }

    #[test]
    fn auto_grid_column_count_fits_minimum_widths_up_to_limits() {
        let DashboardLayoutItem::AutoGrid(grid) = auto_grid(vec![0, 1, 2, 3]) else {
            unreachable!()
        };

        // Each 10-cell column after the first also needs a one-cell gap.
        for (width, columns) in [(0, 1), (10, 1), (20, 1), (21, 2), (32, 3), (500, 3)] {
            assert_eq!(grid.column_count(width), columns, "{width}");
        }
        let two_panels = DashboardAutoGrid {
            panels: vec![0, 1],
            ..grid
        };
        assert_eq!(two_panels.column_count(500), 2);
    }

    #[test]
    fn auto_grid_panels_are_visible_at_the_enclosing_depth() {
        let layout = DashboardLayout::new(vec![
            auto_grid(vec![0, 1]),
            DashboardLayoutItem::Row(DashboardRow::new(
                RowId::new(0),
                "Row",
                false,
                false,
                vec![auto_grid(vec![2])],
            )),
        ]);

        assert_eq!(
            layout.visible_items(),
            [
                VisibleDashboardItem::panel(0, 0),
                VisibleDashboardItem::panel(1, 0),
                VisibleDashboardItem {
                    id: DashboardItemId::Row(RowId::new(0)),
                    depth: 0,
                },
                VisibleDashboardItem::panel(2, 1),
            ]
        );
    }

    #[test]
    fn collapsing_a_row_hides_its_auto_grid_and_selects_the_row() {
        let mut layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(0),
            "Row",
            false,
            false,
            vec![auto_grid(vec![0, 1])],
        ))]);

        layout.set_row_collapsed(RowId::new(0), true);

        assert_eq!(layout.visible_panel_indices(), Vec::<usize>::new());
        assert_eq!(
            layout.nearest_visible_ancestor(DashboardItemId::Panel(1)),
            Some(DashboardItemId::Row(RowId::new(0)))
        );
        assert_eq!(
            layout.set_row_collapsed(RowId::new(0), false),
            Some(LayoutChange {
                newly_visible_panels: vec![0, 1]
            })
        );
    }

    #[test]
    fn flat_layout_lists_each_panel_at_root_depth() {
        let layout = DashboardLayout::flat(3);

        assert_eq!(layout.visible_panel_count(), 3);
        assert_eq!(
            layout.visible_items(),
            vec![
                VisibleDashboardItem::panel(0, 0),
                VisibleDashboardItem::panel(1, 0),
                VisibleDashboardItem::panel(2, 0),
            ]
        );
        assert_eq!(layout.first_visible(), Some(DashboardItemId::Panel(0)));
    }

    #[test]
    fn toggling_a_collapsed_row_reveals_panels_in_source_order() {
        let row = DashboardRow::new(
            RowId::new(1),
            "Collapsed",
            true,
            false,
            vec![DashboardLayoutItem::Panel(3), DashboardLayoutItem::Panel(1)],
        );
        let mut layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(row)]);

        let change = layout.toggle_row(RowId::new(1)).unwrap();

        assert_eq!(change.newly_visible_panels, vec![3, 1]);
        assert_eq!(layout.visible_panel_indices(), vec![3, 1]);
    }

    #[test]
    fn unknown_rows_do_not_change_the_layout() {
        let mut layout = DashboardLayout::flat(1);

        assert_eq!(layout.set_row_collapsed(RowId::new(99), true), None);
        assert_eq!(layout.toggle_row(RowId::new(99)), None);
        assert_eq!(layout.visible_panel_indices(), vec![0]);
    }

    #[test]
    fn nearest_visible_ancestor_prefers_the_target_then_visible_rows_then_first_item() {
        let collapsed_child = DashboardRow::new(
            RowId::new(2),
            "Child",
            true,
            false,
            vec![DashboardLayoutItem::Panel(2)],
        );
        let parent = DashboardRow::new(
            RowId::new(1),
            "Parent",
            false,
            false,
            vec![DashboardLayoutItem::Row(collapsed_child)],
        );
        let layout = DashboardLayout::new(vec![
            DashboardLayoutItem::Panel(0),
            DashboardLayoutItem::Row(parent),
        ]);

        assert_eq!(
            layout.nearest_visible_ancestor(DashboardItemId::Panel(0)),
            Some(DashboardItemId::Panel(0))
        );
        assert_eq!(
            layout.nearest_visible_ancestor(DashboardItemId::Panel(2)),
            Some(DashboardItemId::Row(RowId::new(2)))
        );
        assert_eq!(
            layout.nearest_visible_ancestor(DashboardItemId::Panel(99)),
            Some(DashboardItemId::Panel(0))
        );
    }
}
