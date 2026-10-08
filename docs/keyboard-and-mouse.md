# Keyboard and Mouse

grafana-tui is designed for keyboard-first dashboard inspection.

## Keyboard Controls

| Key | Action |
|---|---|
| `q` | Quit |
| `r` | Force refresh |
| `+` / `-` | Zoom out / in |
| `Shift+Left` / `Shift+Right` | Pan left / right in time |
| `0` | Reset to live mode in fullscreen; show every series in normal mode |
| `Up` / `Down` or `k` / `j` | Select previous or next visible row or panel |
| `Enter` / `Space` | Toggle the selected row, or enter the selected tab bar's active tab |
| `Left` / `Right` | Collapse / expand the selected row, or switch tabs on the selected tab bar |
| `Tab` / `Shift+Tab` | Show the next / previous dashboard, with [several dashboards](grafana-dashboard-import.md#several-dashboards) |
| `PgUp` / `PgDn` | Scroll vertically, or select panels in fullscreen |
| `Home` / `End` | Jump to top or bottom |
| `y` | Toggle Y-axis mode |
| `g` | Toggle autogrid guide lines |
| `a` | Toggle external annotation markers |
| `t` | Open the global annotation tag filter |
| `T` | Open the theme picker |
| `1` through `9` | Toggle series visibility |
| `f` | Toggle fullscreen mode for the selected panel |
| `v` | Toggle value inspection mode |
| `Enter` in inspect mode | Open the selected panel's annotation cluster at the cursor |
| `e` | Export current view |
| `Ctrl+E` | Start or stop changed-frame recording |
| `/` | Search visible rows and panels |
| `Left` / `Right` | Move cursor in inspect mode |
| `?` | Toggle debug info |

> **Known issue**: `[` and `]` are also bound to panning, but only when the
> terminal reports Shift with them, which most terminals do not. Live mode can
> only be restored from fullscreen.

## Mouse Support

| Action | Behavior |
|---|---|
| Click | Select a row or panel; click a row disclosure marker to toggle it; click a tab to show it; move the cursor in fullscreen inspect mode |
| Drag | Move the cursor in fullscreen inspect mode |
| Scroll | Scroll the dashboard vertically |

In normal mode, clicking selects rows or panels. Press `v` or `f` on a selected
panel to use cursor-focused interactions.

## Annotation Modals

The global tag filter opens with `t`. Use `Up`/`Down` or `k`/`j` to move,
`Space` to toggle the highlighted tag, `c` to clear the draft, `Enter` to apply
it, or `Esc` to discard it. In an annotation cluster, use `Up`/`Down` or
`k`/`j` to select an event, `PgUp`/`PgDn` to page, and `Enter` or `Esc` to
close it. Mouse input is ignored while either annotation modal is open.

## Theme Picker

`T` opens the theme picker in any mode except search. Themes are grouped by
family, and user-defined themes appear under Custom. Each row shows the theme's
background and series colors.

Moving the selection with `Up`/`Down`, `k`/`j`, `PgUp`/`PgDn`, `Home`, or
`End` previews that theme immediately. `Enter` keeps it; `Esc`, `T`, or `q`
restores the theme the picker opened with, which is marked `•`. Mouse input is
ignored while the picker is open.

The choice lasts for the current session. To keep it, set `theme` in the
[configuration file](configuration.md#themes).
