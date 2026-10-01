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

//! Grafana V2 conditional rendering: rules that show or hide rows, tabs, and
//! auto grid items depending on variables, the time range, and panel data.
//!
//! Evaluation follows Grafana 13's `dashboard-scene/conditional-rendering`.

use std::collections::{HashMap, HashSet};

use crate::dashboard::{RowId, TabGroupId};

/// A `ConditionalRenderingGroup`: conditions combined with `and` or `or`,
/// showing or hiding its item when they hold.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ConditionGroup {
    /// `visibility: show`; `hide` negates the combined result.
    pub(crate) show: bool,
    /// `condition: and`; `or` needs any one condition to hold.
    pub(crate) match_all: bool,
    pub(crate) conditions: Vec<Condition>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Condition {
    /// `ConditionalRenderingVariable`: compares a variable's selected values.
    Variable {
        name: String,
        operator: VariableOperator,
        value: String,
    },
    /// `ConditionalRenderingData`: whether the item's panel returned data.
    Data { has_data: bool },
    /// `ConditionalRenderingTimeRangeSize`: whether the time range is at most
    /// this many seconds; `None` for a value Grafana cannot parse.
    TimeRangeAtMost(Option<f64>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VariableOperator {
    Equals,
    NotEquals,
    Matches,
    NotMatches,
}

/// An item that conditional rendering can hide, by its displayed id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ConditionTarget {
    Row(RowId),
    /// A tab by its group and position among the group's tabs.
    Tab(TabGroupId, usize),
    /// An auto grid item's panel.
    Panel(usize),
}

/// Conditions of an imported layout, keyed by the ids it was imported with.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Conditions {
    pub(crate) rows: HashMap<RowId, ConditionGroup>,
    pub(crate) tabs: HashMap<(TabGroupId, usize), ConditionGroup>,
    pub(crate) panels: HashMap<usize, ConditionGroup>,
}

/// What conditions are evaluated against.
pub(crate) struct ConditionContext<'a> {
    /// Raw selected values of each defined variable.
    pub(crate) values: &'a HashMap<String, Vec<String>>,
    /// Names of every defined variable, including those without a selection.
    pub(crate) defined: &'a HashSet<String>,
    /// Variables with `All` selected.
    pub(crate) all_selected: &'a HashSet<String>,
    /// Width of the dashboard time range.
    pub(crate) range_seconds: f64,
    /// Whether a panel returned data, or `None` before it has loaded.
    pub(crate) panel_has_data: &'a dyn Fn(usize) -> Option<bool>,
}

impl ConditionGroup {
    /// Whether the group's item is shown. Conditions that cannot be decided,
    /// such as a data condition before its panel loads, are left out; with none
    /// left the item is shown.
    pub(crate) fn shows(
        &self,
        context: &ConditionContext,
        scope: &[(String, String)],
        panel: Option<usize>,
    ) -> bool {
        let results: Vec<bool> = self
            .conditions
            .iter()
            .filter_map(|condition| condition.evaluate(context, scope, panel))
            .collect();
        if results.is_empty() {
            return true;
        }
        let matched = if self.match_all {
            results.iter().all(|result| *result)
        } else {
            results.iter().any(|result| *result)
        };
        matched == self.show
    }

    pub(crate) fn uses_data(&self) -> bool {
        self.conditions
            .iter()
            .any(|condition| matches!(condition, Condition::Data { .. }))
    }
}

impl Condition {
    fn evaluate(
        &self,
        context: &ConditionContext,
        scope: &[(String, String)],
        panel: Option<usize>,
    ) -> Option<bool> {
        match self {
            Condition::Variable {
                name,
                operator,
                value,
            } => evaluate_variable(context, scope, name, *operator, value),
            Condition::Data { has_data } => {
                // Grafana only evaluates data conditions on panels.
                let loaded = (context.panel_has_data)(panel?)?;
                Some(loaded == *has_data)
            }
            Condition::TimeRangeAtMost(seconds) => Some(context.range_seconds <= (*seconds)?),
        }
    }
}

fn evaluate_variable(
    context: &ConditionContext,
    scope: &[(String, String)],
    name: &str,
    operator: VariableOperator,
    value: &str,
) -> Option<bool> {
    // A repeat copy sees its own value, as Grafana's scene variable lookup does.
    let scoped = scope.iter().rev().find(|(variable, _)| variable == name);
    let values: Vec<&str> = match scoped {
        Some((_, value)) => vec![value.as_str()],
        None if !context.defined.contains(name) => return None,
        None => match context.values.get(name) {
            Some(values) if !values.is_empty() => values.iter().map(String::as_str).collect(),
            _ => vec![""],
        },
    };
    let all_selected = scoped.is_none()
        && context.all_selected.contains(name)
        && value.eq_ignore_ascii_case("all");

    let hit = match operator {
        VariableOperator::Equals | VariableOperator::NotEquals => {
            all_selected || values.contains(&value)
        }
        VariableOperator::Matches | VariableOperator::NotMatches => {
            // Grafana shows the item when the pattern is invalid.
            let Ok(regex) = regex::Regex::new(value) else {
                return Some(true);
            };
            values.iter().any(|value| regex.is_match(value))
        }
    };
    Some(match operator {
        VariableOperator::Equals | VariableOperator::Matches => hit,
        VariableOperator::NotEquals | VariableOperator::NotMatches => !hit,
    })
}

/// Parses a `ConditionalRenderingTimeRangeSize` value such as `7d` into seconds.
///
/// Mirrors Grafana's `/^(\d+(?:\.\d+)?)[Mwdhmsy]$/` validation and
/// `rangeUtil.intervalToSeconds`, where a month is 30 days and a year 365.
pub(crate) fn parse_time_range_size(value: &str) -> Option<f64> {
    let unit = value.chars().last()?;
    let number = &value[..value.len() - unit.len_utf8()];
    let valid = !number.is_empty()
        && !number.starts_with('.')
        && !number.ends_with('.')
        && number.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
        && number.matches('.').count() <= 1;
    if !valid {
        return None;
    }
    let seconds = match unit {
        's' => 1.0,
        'm' => 60.0,
        'h' => 3_600.0,
        'd' => 86_400.0,
        'w' => 604_800.0,
        'M' => 2_592_000.0,
        'y' => 31_536_000.0,
        _ => return None,
    };
    Some(number.parse::<f64>().ok()? * seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        values: HashMap<String, Vec<String>>,
        defined: HashSet<String>,
        all_selected: HashSet<String>,
    }

    impl Fixture {
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
            Self {
                defined: values.keys().cloned().collect(),
                values,
                all_selected: HashSet::new(),
            }
        }

        fn context<'a>(
            &'a self,
            range_seconds: f64,
            panel_has_data: &'a dyn Fn(usize) -> Option<bool>,
        ) -> ConditionContext<'a> {
            ConditionContext {
                values: &self.values,
                defined: &self.defined,
                all_selected: &self.all_selected,
                range_seconds,
                panel_has_data,
            }
        }
    }

    fn group(show: bool, match_all: bool, conditions: Vec<Condition>) -> ConditionGroup {
        ConditionGroup {
            show,
            match_all,
            conditions,
        }
    }

    fn variable(name: &str, operator: VariableOperator, value: &str) -> Condition {
        Condition::Variable {
            name: name.to_string(),
            operator,
            value: value.to_string(),
        }
    }

    const NO_DATA: &dyn Fn(usize) -> Option<bool> = &|_| None;

    #[test]
    fn variable_conditions_compare_any_selected_value() {
        let fixture = Fixture::new(&[("env", &["prod", "staging"])]);
        let context = fixture.context(3600.0, NO_DATA);

        for (operator, value, expected) in [
            (VariableOperator::Equals, "prod", true),
            (VariableOperator::Equals, "dev", false),
            (VariableOperator::NotEquals, "prod", false),
            (VariableOperator::NotEquals, "dev", true),
            (VariableOperator::Matches, "^stag", true),
            (VariableOperator::Matches, "(?i)PROD", true),
            (VariableOperator::NotMatches, "dev|test", true),
            // An invalid pattern shows the item, whatever the operator.
            (VariableOperator::NotMatches, "(", true),
        ] {
            let shows = group(true, true, vec![variable("env", operator, value)]).shows(
                &context,
                &[],
                None,
            );
            assert_eq!(shows, expected, "{operator:?} {value}");
        }
    }

    #[test]
    fn all_matches_while_all_is_selected_and_repeat_copies_see_their_value() {
        let mut fixture = Fixture::new(&[("env", &["prod", "staging"])]);
        fixture.all_selected.insert("env".to_string());
        let context = fixture.context(3600.0, NO_DATA);
        let condition = group(
            true,
            true,
            vec![variable("env", VariableOperator::Equals, "All")],
        );

        assert!(condition.shows(&context, &[], None));
        let scope = [("env".to_string(), "prod".to_string())];
        assert!(!condition.shows(&context, &scope, None));
        let prod_only = group(
            true,
            true,
            vec![variable("env", VariableOperator::Equals, "staging")],
        );
        assert!(!prod_only.shows(&context, &scope, None));
    }

    #[test]
    fn groups_combine_ignore_undecided_conditions_and_may_hide() {
        let fixture = Fixture::new(&[("env", &["prod"])]);
        let has_data = |panel: usize| (panel == 1).then_some(true);
        let context = fixture.context(3600.0, &has_data);
        let prod = variable("env", VariableOperator::Equals, "prod");
        let dev = variable("env", VariableOperator::Equals, "dev");

        assert!(!group(true, true, vec![prod.clone(), dev.clone()]).shows(&context, &[], None));
        assert!(group(true, false, vec![prod.clone(), dev.clone()]).shows(&context, &[], None));
        assert!(!group(false, true, vec![prod.clone()]).shows(&context, &[], None));
        assert!(group(true, true, vec![]).shows(&context, &[], None));
        // Undefined variables, unloaded panels, and data conditions on non-panels
        // are undecided, so the item is shown.
        let undecided = vec![
            variable("missing", VariableOperator::Equals, "x"),
            Condition::Data { has_data: false },
        ];
        assert!(group(true, true, undecided.clone()).shows(&context, &[], Some(0)));
        assert!(group(false, true, undecided).shows(&context, &[], None));
        // Panel 1 has data, so "no data" hides it.
        let no_data = group(true, true, vec![Condition::Data { has_data: false }]);
        assert!(!no_data.shows(&context, &[], Some(1)));
    }

    #[test]
    fn time_range_conditions_hold_up_to_their_size() {
        let fixture = Fixture::new(&[]);
        let condition = |value: &str| {
            group(
                true,
                true,
                vec![Condition::TimeRangeAtMost(parse_time_range_size(value))],
            )
        };

        let hour = fixture.context(3600.0, NO_DATA);
        assert!(condition("1h").shows(&hour, &[], None));
        assert!(condition("1.5h").shows(&hour, &[], None));
        assert!(!condition("30m").shows(&hour, &[], None));
        // Unparseable sizes are undecided.
        assert!(condition("1 hour").shows(&hour, &[], None));
    }

    #[test]
    fn time_range_sizes_parse_like_grafana() {
        for (value, seconds) in [
            ("30s", Some(30.0)),
            ("15m", Some(900.0)),
            ("1.5h", Some(5400.0)),
            ("7d", Some(604_800.0)),
            ("2w", Some(1_209_600.0)),
            ("1M", Some(2_592_000.0)),
            ("1y", Some(31_536_000.0)),
            ("h", None),
            ("1.h", None),
            ("1x", None),
            ("-1h", None),
            ("", None),
        ] {
            assert_eq!(parse_time_range_size(value), seconds, "{value}");
        }
    }
}
