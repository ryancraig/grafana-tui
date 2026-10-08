"""Builder for Grafana 13 dashboard v2 resources (dashboard.grafana.app/v2).

Every panel carries the Grafana-side presentation (legend, tooltip, reduce,
table overrides) a real Grafana 13 honours, plus the fieldConfig defaults
grafana-tui renders. Nothing here emits mappings or transformations.
"""
import json
import re

DS = {"name": "${datasource}"}
GREEN, AMBER, RED, BLUE = "green", "orange", "red", "blue"


def steps(*pairs):
    """steps(RED, (14400, AMBER), (86400, GREEN)) -> Grafana threshold steps."""
    base, rest = pairs[0], pairs[1:]
    return {"mode": "absolute", "steps": [{"value": None, "color": base}] + [{"value": v, "color": c} for v, c in rest]}


# Common threshold shapes.
ZERO_GOOD = steps(GREEN, (1, RED))                  # a count of problems
ZERO_GOOD_AMBER = steps(GREEN, (1, AMBER))
ONE_GOOD = steps(RED, (1, GREEN))                   # a count that must be >= 1
UTIL = steps(GREEN, (0.8, AMBER), (0.9, RED))       # utilisation 0..1
NEUTRAL = steps(BLUE)


def q(expr, legend="", ref="A", instant=None, fmt=None):
    spec = {"expr": expr, "legendFormat": legend, "range": not instant}
    if instant is not None:
        spec["instant"] = instant
    if fmt:
        spec["format"] = fmt
    return {"kind": "PanelQuery", "spec": {
        "refId": ref, "hidden": False,
        "query": {"kind": "DataQuery", "group": "prometheus", "version": "v0", "datasource": DS, "spec": spec}}}


class Panel:
    def __init__(self, kind, title, queries, w, h, desc, defaults, options, overrides=None):
        self.kind, self.title, self.queries, self.w, self.h = kind, title, queries, w, h
        self.desc, self.defaults, self.options, self.overrides = desc, defaults, options, overrides or []

    def element(self, pid):
        qs = []
        for i, item in enumerate(self.queries):
            qs.append(q(*item[:2], ref=chr(65 + i), **(item[2] if len(item) > 2 else {})))
        return {"kind": "Panel", "spec": {
            "id": pid, "title": self.title, "description": self.desc, "links": [],
            "data": {"kind": "QueryGroup", "spec": {"queries": qs, "transformations": [], "queryOptions": {}}},
            "vizConfig": {"kind": "VizConfig", "group": self.kind, "version": "13.0.0", "spec": {
                "options": self.options,
                "fieldConfig": {"defaults": self.defaults, "overrides": self.overrides}}}}}


def _defaults(unit, thresholds, decimals, mn, mx, color_mode, custom=None):
    d = {"unit": unit, "color": {"mode": color_mode}, "thresholds": thresholds}
    if decimals is not None:
        d["decimals"] = decimals
    if mn is not None:
        d["min"] = mn
    if mx is not None:
        d["max"] = mx
    if custom:
        d["custom"] = custom
    return d


REDUCE = {"calcs": ["lastNotNull"], "fields": "", "values": False}


def stat(title, expr, unit="short", th=NEUTRAL, desc="", w=4, h=3, decimals=None, mn=None, mx=None, graph=True):
    if decimals is None and unit == "short":
        decimals = 0  # counts
    return Panel("stat", title, [(expr, title)], w, h, desc,
                 _defaults(unit, th, decimals, mn, mx, "thresholds"),
                 {"reduceOptions": REDUCE, "colorMode": "background", "graphMode": "area" if graph else "none",
                  "justifyMode": "center", "textMode": "value", "orientation": "auto", "wideLayout": True,
                  "showPercentChange": False})


def ts(title, queries, unit="short", desc="", w=12, h=7, th=None, mn=0, mx=None, decimals=None,
       fill=10, stack=False, draw="line", legend_calcs=("lastNotNull", "max"), th_style="off"):
    custom = {"drawStyle": draw, "lineWidth": 1, "fillOpacity": fill, "showPoints": "never",
              "lineInterpolation": "linear", "axisPlacement": "auto", "axisGridShow": True,
              "spanNulls": False, "stacking": {"mode": "normal" if stack else "none", "group": "A"},
              "thresholdsStyle": {"mode": th_style}}
    return Panel("timeseries", title, queries, w, h, desc,
                 _defaults(unit, th or steps(GREEN), decimals, mn, mx, "palette-classic", custom),
                 {"legend": {"showLegend": True, "displayMode": "table", "placement": "bottom",
                             "calcs": list(legend_calcs)},
                  "tooltip": {"mode": "multi", "sort": "desc"}})


def bargauge(title, queries, unit="short", th=UTIL, desc="", w=8, h=7, mn=0, mx=None, decimals=None):
    queries = [(e, l, {"instant": True}) for e, l in queries]
    return Panel("bargauge", title, queries, w, h, desc,
                 _defaults(unit, th, decimals, mn, mx, "thresholds"),
                 {"reduceOptions": REDUCE, "orientation": "horizontal", "displayMode": "gradient",
                  "valueMode": "color", "namePlacement": "left", "showUnfilled": True, "sizing": "auto",
                  "minVizHeight": 10, "minVizWidth": 0, "maxVizHeight": 300})


def gauge(title, expr, unit="percentunit", th=UTIL, desc="", w=4, h=4, mn=0, mx=1, decimals=None):
    return Panel("gauge", title, [(expr, title, {"instant": True})], w, h, desc,
                 _defaults(unit, th, decimals, mn, mx, "thresholds"),
                 {"reduceOptions": REDUCE, "orientation": "auto", "showThresholdLabels": False,
                  "showThresholdMarkers": True, "sizing": "auto", "minVizHeight": 75, "minVizWidth": 75})


def table(title, queries, unit="short", th=NEUTRAL, desc="", w=12, h=7, decimals=None, sort_desc=True):
    queries = [(e, l, {"instant": True, "fmt": "table"}) for e, l in queries]
    hide_time = {"matcher": {"id": "byName", "options": "Time"},
                 "properties": [{"id": "custom.hidden", "value": True}]}
    color_value = {"matcher": {"id": "byName", "options": "Value"},
                   "properties": [{"id": "custom.cellOptions", "value": {"type": "color-background", "mode": "basic"}}]}
    return Panel("table", title, queries, w, h, desc,
                 _defaults(unit, th, decimals, None, None, "thresholds",
                           {"align": "auto", "cellOptions": {"type": "auto"}, "inspect": False, "filterable": True}),
                 {"showHeader": True, "cellHeight": "sm", "footer": {"show": False, "reducer": ["sum"], "fields": ""},
                  "sortBy": [{"displayName": "Value", "desc": sort_desc}]},
                 [hide_time, color_value])


def query_var(name, label, query, multi=True, include_all=True, regex=""):
    return {"kind": "QueryVariable", "spec": {
        "name": name, "label": label,
        "current": {"text": "All", "value": "$__all"} if include_all else {"text": "", "value": ""},
        "hide": "dontHide", "refresh": "onTimeRangeChanged", "skipUrlSync": False,
        "query": {"kind": "DataQuery", "group": "prometheus", "version": "v0", "datasource": DS,
                  "spec": {"query": query}},
        "definition": query, "regex": regex, "sort": "alphabeticalAsc", "options": [],
        "multi": multi, "includeAll": include_all, "allValue": ".*", "allowCustomValue": True}}


DATASOURCE_VAR = {"kind": "DatasourceVariable", "spec": {
    "name": "datasource", "label": "Data source", "pluginId": "prometheus", "refresh": "onDashboardLoad",
    "regex": "", "current": {"text": "Prometheus", "value": "prometheus"}, "options": [],
    "multi": False, "includeAll": False, "hide": "dontHide", "skipUrlSync": False, "allowCustomValue": True}}


def _slug(s):
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")


def dashboard(name, title, desc, tags, rows, variables=(), refresh="30s", time_from="now-3h"):
    """rows: [(row title, [Panel, ...]), ...]; panels pack left-to-right, wrapping at 24 columns."""
    elements, out_rows, pid = {}, [], 0
    for row_title, panels in rows:
        items, x, y, line_h = [], 0, 0, 0
        for p in panels:
            if x + p.w > 24:
                x, y, line_h = 0, y + line_h, 0
            pid += 1
            key = f"p{pid:02d}-{_slug(p.title)}"[:60]
            elements[key] = p.element(pid)
            items.append({"kind": "GridLayoutItem", "spec": {
                "x": x, "y": y, "width": p.w, "height": p.h,
                "element": {"kind": "ElementReference", "name": key}}})
            x += p.w
            line_h = max(line_h, p.h)
        out_rows.append({"kind": "RowsLayoutRow", "spec": {
            "title": row_title, "collapse": False, "hideHeader": False,
            "layout": {"kind": "GridLayout", "spec": {"items": items}}}})
    return {
        "apiVersion": "dashboard.grafana.app/v2",
        "kind": "Dashboard",
        "metadata": {"name": name},
        "spec": {
            "title": title,
            "description": desc,
            "tags": ["hashistack", "rdw", "gcloud-dc"] + list(tags),
            "editable": True,
            "preload": False,
            "liveNow": False,
            "cursorSync": "Crosshair",
            "links": [],
            "annotations": [],
            "timeSettings": {"from": time_from, "to": "now", "autoRefresh": refresh, "timezone": "browser",
                             "hideTimepicker": False, "fiscalYearStartMonth": 0, "weekStart": "",
                             "autoRefreshIntervals": ["15s", "30s", "1m", "5m", "15m"]},
            "variables": [DATASOURCE_VAR] + list(variables),
            "elements": elements,
            "layout": {"kind": "RowsLayout", "spec": {"rows": out_rows}},
        },
    }


def write(path, dash):
    with open(path, "w") as f:
        json.dump(dash, f, indent=2)
        f.write("\n")


def empty_ok(panel, text="none (healthy)"):
    """Mark a panel whose empty result is the healthy state: Grafana shows `text` instead of 'No data'."""
    panel.defaults["noValue"] = text
    return panel
