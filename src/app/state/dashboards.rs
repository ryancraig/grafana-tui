//! Several dashboards in one session. Each dashboard is a tab of one tab
//! group pinned above the content, and only the active one queries
//! Prometheus: its panels, as for any tab, and its variables, which are its
//! tab's variables.

use super::AppState;
use crate::app::template::SectionInstance;
use crate::dashboard::{DashboardItemId, SectionId, TabGroupId};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// One dashboard of several loaded together.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DashboardInfo {
    /// The dashboard's own title, for the header.
    pub(crate) title: String,
    pub(crate) path: PathBuf,
    /// How often it refreshes while it is the active dashboard.
    pub(crate) refresh_every: Duration,
    /// Panels its import skipped.
    pub(crate) skipped_panels: usize,
}

/// Dashboards loaded together, one tab each in the tab group `group`.
#[derive(Debug, Clone)]
pub(crate) struct DashboardSet {
    pub(crate) group: TabGroupId,
    pub(crate) dashboards: Vec<DashboardInfo>,
    /// The dashboard each row and tab section belongs to, by imported id.
    section_owner: HashMap<SectionId, usize>,
    /// Where each dashboard was left, to return there.
    views: Vec<DashboardView>,
}

#[derive(Debug, Clone, Copy, Default)]
struct DashboardView {
    selected_item: Option<DashboardItemId>,
    vertical_scroll: usize,
}

impl DashboardSet {
    pub(crate) fn new(
        group: TabGroupId,
        dashboards: Vec<DashboardInfo>,
        section_owner: HashMap<SectionId, usize>,
    ) -> Self {
        let views = vec![DashboardView::default(); dashboards.len()];
        Self {
            group,
            dashboards,
            section_owner,
            views,
        }
    }
}

/// Where the selection goes when another dashboard becomes active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DashboardFocus {
    /// The dashboard tab bar, as after clicking a tab.
    Bar,
    /// Where the dashboard was left, or its first item.
    Content,
}

/// Tab labels for dashboard titles: without a `"Prefix / "` that every title
/// shares, so "HashiStack / Nodes" and "HashiStack / Vault" become "Nodes" and
/// "Vault".
pub(crate) fn dashboard_tab_labels(titles: &[String]) -> Vec<String> {
    let prefix = titles.first().and_then(|first| {
        let end = first.rfind(" / ")? + " / ".len();
        let prefix = &first[..end];
        titles
            .iter()
            .all(|title| title.len() > prefix.len() && title.starts_with(prefix))
            .then_some(prefix)
    });
    titles
        .iter()
        .map(|title| prefix.map_or(title.as_str(), |prefix| &title[prefix.len()..]))
        .map(str::to_string)
        .collect()
}

impl AppState {
    /// Shows several dashboards, one per tab of `set.group`, starting with the
    /// first. Call after the template is applied.
    pub(crate) fn set_dashboards(&mut self, set: DashboardSet) {
        debug_assert_eq!(
            self.layout.tabs(set.group).map(|group| group.tabs.len()),
            Some(set.dashboards.len())
        );
        let group = set.group;
        self.dashboards = Some(set);
        self.apply_dashboard_info(0);
        self.selected_item = self
            .layout
            .first_tab_descendant(group)
            .or(Some(DashboardItemId::Tabs(group)));
        self.vertical_scroll = 0;
    }

    /// The tab group holding one tab per dashboard, with several dashboards.
    pub(crate) fn dashboard_group(&self) -> Option<TabGroupId> {
        self.dashboards.as_ref().map(|set| set.group)
    }

    /// What an empty tab of group `group` says.
    pub(crate) fn empty_tab_message(&self, group: TabGroupId) -> &'static str {
        if self.dashboard_group() == Some(group) {
            "No supported panels in this dashboard"
        } else {
            "No supported panels in this tab"
        }
    }

    /// The index of the dashboard shown, with several dashboards.
    pub(crate) fn active_dashboard(&self) -> Option<usize> {
        self.layout.tabs(self.dashboard_group()?)?.active
    }

    /// Shows the next dashboard, or the previous for a negative `direction`,
    /// wrapping around.
    pub(crate) fn cycle_dashboard(&mut self, direction: isize) -> Result<()> {
        let (Some(set), Some(active)) = (&self.dashboards, self.active_dashboard()) else {
            return Ok(());
        };
        let count = set.dashboards.len() as isize;
        let next = (active as isize + direction).rem_euclid(count) as usize;
        self.activate_dashboard(next, DashboardFocus::Content)
    }

    /// Shows dashboard `index`. Only it queries Prometheus from now on, and it
    /// refreshes at once, reloading its variables as when a dashboard opens.
    pub(crate) fn activate_dashboard(&mut self, index: usize, focus: DashboardFocus) -> Result<()> {
        let (Some(group), Some(previous)) = (self.dashboard_group(), self.active_dashboard())
        else {
            return Ok(());
        };
        let count = self
            .dashboards
            .as_ref()
            .map_or(0, |set| set.dashboards.len());
        if index == previous || index >= count {
            if focus == DashboardFocus::Bar {
                self.selected_item = Some(DashboardItemId::Tabs(group));
            }
            return Ok(());
        }
        let view = DashboardView {
            selected_item: self.selected_item,
            vertical_scroll: self.vertical_scroll,
        };
        if let Some(set) = &mut self.dashboards {
            set.views[previous] = view;
        }
        self.layout.set_active_tab(group, index);
        // Panels kept fetching by data conditions are found in the
        // unfiltered layout, so it must show the same dashboard.
        if self.template.is_some() {
            self.unfiltered_layout
                .sync_state_from(&self.layout, &self.hidden_items);
        }
        self.apply_dashboard_info(index);
        self.refreshes.dashboard_switched();

        match focus {
            DashboardFocus::Bar => self.selected_item = Some(DashboardItemId::Tabs(group)),
            DashboardFocus::Content => {
                let view = self
                    .dashboards
                    .as_ref()
                    .map(|set| set.views[index])
                    .unwrap_or_default();
                self.selected_item = view
                    .selected_item
                    .or_else(|| self.layout.first_tab_descendant(group));
                self.vertical_scroll = view.vertical_scroll;
            }
        }
        self.ensure_selection_visible();
        self.start_refresh();
        self.reconcile_visible_annotation_targets();
        Ok(())
    }

    /// Shows dashboard `index`'s title and refreshes at its interval.
    fn apply_dashboard_info(&mut self, index: usize) {
        let Some(info) = self
            .dashboards
            .as_ref()
            .and_then(|set| set.dashboards.get(index))
        else {
            return;
        };
        self.title = format!("{} (imported)", info.title);
        self.skipped_panels = info.skipped_panels;
        self.refresh_every = info.refresh_every;
    }

    /// The row and tab copies whose variables are queried: all of them, except
    /// those of dashboards that are not shown.
    pub(super) fn live_section_instances(&self) -> Vec<SectionInstance> {
        let Some(set) = &self.dashboards else {
            return self.section_instances.clone();
        };
        let active = self.active_dashboard();
        self.section_instances
            .iter()
            .filter(|instance| {
                set.section_owner
                    .get(&instance.id)
                    .is_none_or(|&owner| Some(owner) == active)
            })
            .cloned()
            .collect()
    }

    /// Panels of the dashboard shown, with several dashboards, including those
    /// in its collapsed rows, inactive tabs, or hidden by conditions.
    pub(super) fn active_dashboard_panel_indices(&self) -> Option<Vec<usize>> {
        let group = self.dashboard_group()?;
        let active = self.active_dashboard()?;
        Some(self.unfiltered_layout.tab_panel_indices(group, active))
    }
}

/// Several dashboards loaded as `main` loads them, for tests.
#[cfg(test)]
pub(crate) mod test_support {
    use super::AppState;
    use crate::export::ExportOptions;
    use crate::prom;
    use crate::theme::Theme;
    use std::time::Duration;

    /// A V2 dashboard with one range panel querying `expr` and an `instance`
    /// variable resolved by `instance_query`, refreshing every `auto_refresh`.
    pub(crate) fn v2_dashboard(
        title: &str,
        auto_refresh: &str,
        expr: &str,
        instance_query: &str,
    ) -> String {
        serde_json::json!({
            "apiVersion": "dashboard.grafana.app/v2",
            "kind": "Dashboard",
            "spec": {
                "title": title,
                "elements": {"panel-1": {"kind": "Panel", "spec": {
                    "id": 1,
                    "title": "Panel",
                    "data": {"kind": "QueryGroup", "spec": {"queries": [{
                        "kind": "PanelQuery",
                        "spec": {"refId": "A", "query": {
                            "kind": "DataQuery", "group": "prometheus", "spec": {"expr": expr}
                        }}
                    }]}},
                    "vizConfig": {"kind": "VizConfig", "group": "timeseries", "spec": {}}
                }}},
                "layout": {"kind": "GridLayout", "spec": {"items": [{
                    "kind": "GridLayoutItem",
                    "spec": {"x": 0, "y": 0, "width": 24, "height": 8,
                             "element": {"kind": "ElementReference", "name": "panel-1"}}
                }]}},
                "variables": [{"kind": "QueryVariable", "spec": {
                    "name": "instance",
                    "query": {"kind": "DataQuery", "group": "prometheus",
                              "spec": {"query": instance_query}}
                }}],
                "timeSettings": {"from": "now-6h", "to": "now", "autoRefresh": auto_refresh}
            }
        })
        .to_string()
    }

    /// The app `main` builds for several dashboards.
    pub(crate) fn dashboards_app(url: &str, documents: &[&str]) -> AppState {
        let set = crate::grafana::parse_grafana_dashboards(documents).unwrap();
        let setup = crate::build_dashboard_set(set, None, &[], None, None);
        let mut app = AppState::new(
            prom::PromClient::new(url.to_string()),
            Duration::from_secs(300),
            Duration::from_secs(5),
            Duration::from_secs(1),
            "grafana-tui".to_string(),
            setup.panels,
            0,
            Theme::default(),
            "dashed".to_string(),
            ExportOptions::default(),
        );
        app.variable_names = setup.variables.names;
        app.apply_template(setup.template);
        app.set_dashboards(setup.set);
        app
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_labels_drop_a_shared_title_prefix() {
        let titles = |titles: &[&str]| titles.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        assert_eq!(
            dashboard_tab_labels(&titles(&["HashiStack / Nodes", "HashiStack / Vault"])),
            ["Nodes", "Vault"]
        );
        // Kept when not every title shares it, or a label would be empty.
        assert_eq!(
            dashboard_tab_labels(&titles(&["HashiStack / Nodes", "Prometheus"])),
            ["HashiStack / Nodes", "Prometheus"]
        );
        assert_eq!(
            dashboard_tab_labels(&titles(&["A / B", "A / "])),
            ["A / B", "A / "]
        );
        assert_eq!(dashboard_tab_labels(&titles(&["Nodes"])), ["Nodes"]);
    }
}
