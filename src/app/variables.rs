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

use super::data::expand_expr;
use crate::grafana::TemplateQueryVar;
use crate::prom;
use anyhow::{Result, anyhow};
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::time::Duration;

/// Replaces `$name` and `${name}` references whose name `lookup` resolves.
///
/// Names are read greedily, as Grafana's `\$(\w+)` does, so `$job_name` never
/// matches a variable called `job`. Unresolved references, and `${name:format}`
/// references, are copied unchanged. Substituted values are not scanned again.
pub(crate) fn substitute_variables<'a>(
    text: &str,
    mut lookup: impl FnMut(&str) -> Option<Cow<'a, str>>,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find('$') {
        out.push_str(&rest[..position]);
        let after = &rest[position + 1..];
        let (name, consumed) = if let Some(braced) = after.strip_prefix('{') {
            match braced.find('}') {
                Some(end) => (&braced[..end], end + 2),
                None => ("", 0),
            }
        } else {
            let end = after
                .find(|ch: char| !is_variable_name_char(ch))
                .unwrap_or(after.len());
            (&after[..end], end)
        };
        let value = (!name.is_empty() && name.chars().all(is_variable_name_char))
            .then(|| lookup(name))
            .flatten();
        match value {
            Some(value) => {
                out.push_str(&value);
                rest = &after[consumed..];
            }
            None => {
                out.push('$');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn is_variable_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// Formats selected variable values for a PromQL expression.
///
/// Mirrors Grafana's Prometheus `interpolateQueryExpr` (classic escaping): a
/// variable that is neither multi-value nor include-all is inserted verbatim;
/// otherwise each value is regex-escaped for use inside a PromQL string, and
/// several values become an alternation such as `(a|b)`.
pub(crate) fn format_prometheus_values(values: &[String], regex: bool) -> String {
    match values {
        [] => String::new(),
        [value] if !regex => value.clone(),
        [value] => prometheus_regex_escape(value),
        values => format!(
            "({})",
            values
                .iter()
                .map(|value| prometheus_regex_escape(value))
                .collect::<Vec<_>>()
                .join("|")
        ),
    }
}

/// Grafana's `prometheusSpecialRegexEscape`: backslashes become four backslashes
/// and regex metacharacters gain two, so they survive PromQL string unescaping.
fn prometheus_regex_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\\\\\"),
            '$' | '^' | '*' | '{' | '}' | '[' | ']' | '+' | '?' | '.' | '(' | ')' | '|' => {
                out.push_str("\\\\");
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Parses a custom variable's option values.
///
/// The CSV form follows Grafana's `CustomVariable`: commas separate options unless
/// escaped as `\,`, and `text : value` entries contribute their value. The JSON
/// form (`valuesFormat: json`) is an array of strings or `{text, value}` objects.
pub(crate) fn parse_custom_variable_values(query: &str, json: bool) -> Vec<String> {
    if json {
        return serde_json::from_str::<Vec<serde_json::Value>>(query)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|option| match option {
                serde_json::Value::String(value) => Some(value),
                serde_json::Value::Object(option) => option
                    .get("value")
                    .or_else(|| option.get("text"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                _ => None,
            })
            .collect();
    }

    let mut values = Vec::new();
    let mut current = String::new();
    let mut chars = query.chars().peekable();
    let mut push = |current: &mut String| {
        let text = current.trim();
        let value = text.rsplit_once(" : ").map_or(text, |(_, value)| value.trim());
        if !value.is_empty() {
            values.push(value.to_string());
        }
        current.clear();
    };
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if chars.peek() == Some(&',') => {
                current.push(',');
                chars.next();
            }
            ',' => push(&mut current),
            _ => current.push(ch),
        }
    }
    push(&mut current);
    values
}

enum PrometheusVariableQuery {
    LabelValues {
        metric: Option<String>,
        label: String,
    },
    QueryResult(String),
}

pub(crate) async fn refresh_query_variables(
    prometheus: &prom::PromClient,
    query_vars: &[TemplateQueryVar],
    range: Duration,
    step: Duration,
    end_ts: i64,
    vars: &mut HashMap<String, String>,
    var_values: &mut HashMap<String, Vec<String>>,
) -> Result<()> {
    for query_var in query_vars {
        let values = resolve_query_variable(prometheus, query_var, range, step, end_ts, vars)
            .await?
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        if values.is_empty() {
            continue;
        }

        let selected = if query_var.select_all {
            values
        } else {
            // Keep a saved selection that is still offered, as Grafana does on load;
            // otherwise select the first value.
            match var_values.get(&query_var.name) {
                Some(selected)
                    if !selected.is_empty()
                        && selected.iter().all(|value| values.contains(value)) =>
                {
                    selected.clone()
                }
                _ => values.into_iter().take(1).collect(),
            }
        };
        let formatted = match query_var.all_value.as_ref() {
            Some(all_value) if query_var.select_all => all_value.clone(),
            _ => format_prometheus_values(&selected, query_var.regex_values),
        };
        vars.insert(query_var.name.clone(), formatted);
        var_values.insert(query_var.name.clone(), selected);
    }

    Ok(())
}

async fn resolve_query_variable(
    prometheus: &prom::PromClient,
    query_var: &TemplateQueryVar,
    range: Duration,
    step: Duration,
    end_ts: i64,
    vars: &HashMap<String, String>,
) -> Result<Vec<String>> {
    let expanded_query = expand_expr(&query_var.query, range, step, vars);
    let query = parse_prometheus_variable_query(&expanded_query)?;
    let start_ts = end_ts - range.as_secs() as i64;
    let values = match query {
        PrometheusVariableQuery::LabelValues { metric, label } => {
            if let Some(metric) = metric {
                prometheus
                    .series_label_values(&metric, &label, start_ts, end_ts)
                    .await?
            } else {
                prometheus.label_values(&label).await?
            }
        }
        PrometheusVariableQuery::QueryResult(query) => {
            prometheus
                .query_instant_result_strings(&query, end_ts)
                .await?
        }
    };

    apply_regex(values, query_var.regex.as_deref())
}

fn parse_prometheus_variable_query(query: &str) -> Result<PrometheusVariableQuery> {
    if let Some(args) = call_args(query, "label_values") {
        let args = split_top_level_args(args);
        return match args.as_slice() {
            [label] => Ok(PrometheusVariableQuery::LabelValues {
                metric: None,
                label: label.trim().to_string(),
            }),
            [metric, label] => Ok(PrometheusVariableQuery::LabelValues {
                metric: Some(metric.trim().to_string()),
                label: label.trim().to_string(),
            }),
            _ => Err(anyhow!("unsupported label_values query: {}", query)),
        };
    }

    if let Some(args) = call_args(query, "query_result") {
        return Ok(PrometheusVariableQuery::QueryResult(
            args.trim().to_string(),
        ));
    }

    Ok(PrometheusVariableQuery::QueryResult(
        query.trim().to_string(),
    ))
}

fn call_args<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    let query = query.trim();
    let rest = query.strip_prefix(name)?.trim_start();
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
    Some(inner)
}

fn split_top_level_args(args: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;

    for (idx, ch) in args.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(args[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(args[start..].trim());
    parts.into_iter().filter(|part| !part.is_empty()).collect()
}

fn apply_regex(values: Vec<String>, regex: Option<&str>) -> Result<Vec<String>> {
    let Some(regex) = regex.map(str::trim).filter(|regex| !regex.is_empty()) else {
        return Ok(values);
    };

    let pattern = regex
        .strip_prefix('/')
        .and_then(|regex| regex.rsplit_once('/').map(|(pattern, _)| pattern))
        .unwrap_or(regex);
    let regex = Regex::new(pattern)?;

    Ok(values
        .into_iter()
        .filter_map(|value| {
            let captures = regex.captures(&value)?;
            captures
                .get(1)
                .or_else(|| captures.get(0))
                .map(|matched| matched.as_str().to_string())
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_label_values_query() {
        let PrometheusVariableQuery::LabelValues { metric, label } =
            parse_prometheus_variable_query("label_values(up{job=~\"$job\"}, instance)").unwrap()
        else {
            panic!("expected label_values query");
        };

        assert_eq!(metric.as_deref(), Some("up{job=~\"$job\"}"));
        assert_eq!(label, "instance");
    }

    #[test]
    fn test_parse_label_values_without_metric() {
        let PrometheusVariableQuery::LabelValues { metric, label } =
            parse_prometheus_variable_query("label_values(model_name)").unwrap()
        else {
            panic!("expected label_values query");
        };

        assert!(metric.is_none());
        assert_eq!(label, "model_name");
    }

    #[test]
    fn test_regex_extracts_first_capture_group() {
        let values = vec![
            r#"{instance="node-2", job="node"} 1"#.to_string(),
            r#"{instance="node-1", job="node"} 1"#.to_string(),
        ];

        let values = apply_regex(values, Some(r#"/instance="([^"]+)"/"#)).unwrap();

        assert_eq!(values, ["node-2", "node-1"]);
    }
}
