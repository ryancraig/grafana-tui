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

mod builtin;
mod custom;

use anyhow::{Result, bail};
use ratatui::style::Color;

pub(crate) use builtin::{DEFAULT_THEME, builtin, builtin_family, builtin_names};
pub(crate) use custom::{ThemeSpec, custom_themes};

/// Semantic UI colors. Renderers pick a role, never a literal color, so every
/// theme restyles the whole UI and the SVG export.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Theme {
    pub(crate) name: String,
    /// Fill behind the whole frame.
    pub(crate) background: Color,
    /// Fill behind popups and modals.
    pub(crate) surface: Color,
    pub(crate) text: Color,
    /// Hints and other secondary text.
    pub(crate) text_muted: Color,
    pub(crate) title: Color,
    pub(crate) border: Color,
    pub(crate) border_focused: Color,
    pub(crate) selection_fg: Color,
    pub(crate) selection_bg: Color,
    pub(crate) error: Color,
    pub(crate) warning: Color,
    pub(crate) success: Color,
    pub(crate) axis: Color,
    /// Automatic grid lines and labels, unless `autogrid_color` overrides it.
    pub(crate) grid: Color,
    /// Inspect-mode cursor line.
    pub(crate) cursor: Color,
    pub(crate) gauge_track: Color,
    /// Heatmap low, mid and high bands.
    pub(crate) heatmap: [Color; 3],
    /// Heatmap cells without a finite value.
    pub(crate) heatmap_empty: Color,
    pub(crate) annotation: Color,
    /// Threshold steps whose dashboard color has no terminal equivalent.
    pub(crate) threshold_default: Color,
    /// Series colors. Never empty.
    pub(crate) palette: Vec<Color>,
}

impl Default for Theme {
    fn default() -> Self {
        builtin(DEFAULT_THEME).expect("the default theme is built in")
    }
}

/// Every selectable theme: the built-ins in display order, each replaced by a
/// user theme of the same name, followed by the remaining user themes.
pub(crate) fn catalog(custom: &[Theme]) -> Vec<Theme> {
    let find_custom =
        |name: &str| custom.iter().find(|theme| theme.name.eq_ignore_ascii_case(name));
    let mut themes: Vec<Theme> = builtin_names()
        .map(|name| {
            find_custom(name)
                .cloned()
                .unwrap_or_else(|| builtin(name).expect("listed builtins resolve"))
        })
        .collect();
    for theme in custom {
        if !builtin_names().any(|name| name.eq_ignore_ascii_case(&theme.name)) {
            themes.push(theme.clone());
        }
    }
    themes
}

/// Finds a theme in `catalog` by name or built-in alias, ignoring case.
pub(crate) fn resolve(name: &str, catalog: &[Theme]) -> Result<Theme> {
    let find = |name: &str| {
        catalog
            .iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
    };
    match find(name).or_else(|| builtin::alias_target(name).and_then(find)) {
        Some(theme) => Ok(theme.clone()),
        None => bail!(
            "unknown theme `{name}`; available themes: {}",
            catalog
                .iter()
                .map(|theme| theme.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

impl Theme {

    /// Replaces a threshold step color the terminal cannot show.
    pub(crate) fn threshold_color(&self, color: Color) -> Color {
        if color == Color::Reset {
            self.threshold_default
        } else {
            color
        }
    }
}

/// The ANSI theme: follows the terminal's own palette and background.
fn terminal() -> Theme {
    Theme {
        name: "terminal".to_string(),
        background: Color::Reset,
        surface: Color::Reset,
        text: Color::Reset,
        text_muted: Color::DarkGray,
        title: Color::Cyan,
        border: Color::DarkGray,
        border_focused: Color::Yellow,
        selection_fg: Color::Cyan,
        selection_bg: Color::Reset,
        error: Color::Red,
        warning: Color::Yellow,
        success: Color::Green,
        axis: Color::Gray,
        grid: Color::DarkGray,
        cursor: Color::White,
        gauge_track: Color::DarkGray,
        heatmap: [Color::Cyan, Color::Yellow, Color::Red],
        heatmap_empty: Color::DarkGray,
        annotation: Color::Yellow,
        threshold_default: Color::Reset,
        palette: vec![
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Red,
            Color::LightGreen,
            Color::LightYellow,
            Color::LightBlue,
            Color::LightMagenta,
            Color::LightCyan,
            Color::LightRed,
        ],
    }
}

/// The handful of colors a palette-based theme is built from.
#[derive(Debug, Clone, Copy)]
struct Base {
    bg: u32,
    /// Popups and modals.
    bg_alt: u32,
    /// Selected rows and gauge tracks.
    highlight: u32,
    /// Grid lines and empty heatmap cells.
    subtle: u32,
    border: u32,
    muted: u32,
    fg: u32,
    title: u32,
    focus: u32,
    red: u32,
    orange: u32,
    yellow: u32,
    green: u32,
    cyan: u32,
    blue: u32,
    purple: u32,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl Theme {
    fn from_base(name: &str, base: &Base) -> Self {
        let mut palette = Vec::new();
        for hex in [
            base.blue,
            base.green,
            base.yellow,
            base.purple,
            base.cyan,
            base.orange,
            base.red,
        ] {
            let color = rgb(hex);
            if !palette.contains(&color) {
                palette.push(color);
            }
        }
        Self {
            name: name.to_string(),
            background: rgb(base.bg),
            surface: rgb(base.bg_alt),
            text: rgb(base.fg),
            text_muted: rgb(base.muted),
            title: rgb(base.title),
            border: rgb(base.border),
            border_focused: rgb(base.focus),
            selection_fg: rgb(base.title),
            selection_bg: rgb(base.highlight),
            error: rgb(base.red),
            warning: rgb(base.yellow),
            success: rgb(base.green),
            axis: rgb(base.muted),
            grid: rgb(base.subtle),
            cursor: rgb(base.fg),
            gauge_track: rgb(base.highlight),
            heatmap: [rgb(base.blue), rgb(base.yellow), rgb(base.red)],
            heatmap_empty: rgb(base.subtle),
            annotation: rgb(base.focus),
            threshold_default: rgb(base.orange),
            palette,
        }
    }
}

pub(crate) fn parse_grafana_color(c: &str) -> Color {
    if c.starts_with('#') && c.len() >= 7 {
        let r = u8::from_str_radix(&c[1..3], 16).unwrap_or(0);
        let g = u8::from_str_radix(&c[3..5], 16).unwrap_or(0);
        let b = u8::from_str_radix(&c[5..7], 16).unwrap_or(0);
        return Color::Rgb(r, g, b);
    }

    match c.to_lowercase().as_str() {
        "green" | "dark-green" => Color::Green,
        "super-light-green" | "light-green" => Color::LightGreen,
        "yellow" | "dark-yellow" => Color::Yellow,
        "super-light-yellow" | "light-yellow" => Color::LightYellow,
        "red" | "dark-red" => Color::Red,
        "super-light-red" | "light-red" => Color::LightRed,
        "blue" | "dark-blue" => Color::Blue,
        "super-light-blue" | "light-blue" => Color::LightBlue,
        "purple" | "dark-purple" => Color::Magenta,
        "super-light-purple" | "light-purple" => Color::LightMagenta,
        "orange" | "dark-orange" => Color::Rgb(255, 165, 0),
        "light-orange" => Color::Rgb(255, 200, 100),
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "dark-gray" | "dark-grey" => Color::DarkGray,
        "white" => Color::White,
        "black" => Color::Black,
        _ => Color::Reset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_resolves_with_a_usable_palette() {
        for name in builtin_names() {
            let theme = builtin(name).unwrap_or_else(|| panic!("{name} should resolve"));
            assert_eq!(theme.name, name);
            assert!(!theme.palette.is_empty(), "{name} palette is empty");
            if name != "terminal" {
                assert_ne!(theme.background, Color::Reset, "{name} paints no background");
                assert_ne!(theme.text, Color::Reset, "{name} has no text color");
            }
        }
    }

    #[test]
    fn lookup_ignores_case() {
        assert_eq!(builtin("Dracula").map(|t| t.name), Some("dracula".to_string()));
    }

    #[test]
    fn unknown_names_are_rejected_with_the_available_names() {
        let error = resolve("nope", &catalog(&[])).unwrap_err().to_string();
        assert!(error.contains("unknown theme `nope`"), "{error}");
        assert!(error.contains("catppuccin-latte"), "{error}");
        assert!(error.contains("gruvbox-light-soft"), "{error}");
    }

    #[test]
    fn aliases_resolve_to_canonical_flavors() {
        for (alias, canonical) in [
            ("default", "tokyo-night"),
            ("tokyo-night-night", "tokyo-night"),
            ("catppuccin", "catppuccin-mocha"),
            ("Catppuccin", "catppuccin-mocha"),
            ("gruvbox", "gruvbox-dark"),
            ("gruvbox-dark-medium", "gruvbox-dark"),
            ("gruvbox-light-medium", "gruvbox-light"),
        ] {
            assert_eq!(resolve(alias, &catalog(&[])).unwrap().name, canonical, "{alias}");
        }
        assert_eq!(Theme::default().name, DEFAULT_THEME);
    }

    #[test]
    fn user_themes_shadow_builtins_in_place_and_append_the_rest() {
        let shadow = Theme {
            name: "gruvbox-dark".to_string(),
            title: Color::Rgb(1, 2, 3),
            ..builtin("gruvbox-dark").unwrap()
        };
        let extra = Theme {
            name: "Mine".to_string(),
            ..Theme::default()
        };
        let themes = catalog(&[shadow.clone(), extra.clone()]);

        let names: Vec<_> = themes.iter().map(|theme| theme.name.as_str()).collect();
        let builtins: Vec<_> = builtin_names().collect();
        assert_eq!(names[..builtins.len()], builtins[..]);
        assert_eq!(names[builtins.len()..], ["Mine"]);
        assert_eq!(resolve("gruvbox-dark", &themes).unwrap(), shadow);
        // Aliases reach the user's override of their target.
        assert_eq!(resolve("gruvbox", &themes).unwrap(), shadow);
        assert_eq!(resolve("mine", &themes).unwrap(), extra);
        assert!(resolve("nope", &themes).unwrap_err().to_string().contains("Mine"));
    }

    #[test]
    fn every_builtin_has_a_family() {
        for name in builtin_names() {
            assert!(builtin_family(name).is_some(), "{name}");
        }
        assert_eq!(builtin_family("Catppuccin-Latte"), Some("Catppuccin"));
        assert_eq!(builtin_family("mine"), None);
    }

    #[test]
    fn aliases_point_at_builtins_without_shadowing_them() {
        for (alias, target) in builtin::ALIASES {
            assert!(builtin_names().any(|name| name == *target), "{alias} -> {target}");
            assert!(builtin_names().all(|name| name != *alias), "{alias} shadows a builtin");
        }
    }

    #[test]
    fn light_flavors_paint_light_backgrounds() {
        let luma = |color| match color {
            Color::Rgb(r, g, b) => 0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b),
            other => panic!("expected rgb, got {other:?}"),
        };
        for name in builtin_names().filter(|name| *name != "terminal") {
            let theme = builtin(name).unwrap();
            let light = ["day", "latte", "light"].iter().any(|flavor| name.contains(flavor));
            assert_eq!(luma(theme.background) > 128.0, light, "{name}");
            // Text must contrast with the background it sits on.
            assert!((luma(theme.text) - luma(theme.background)).abs() > 80.0, "{name}");
        }
    }

    #[test]
    fn derived_palettes_skip_duplicate_accents() {
        let base = Base {
            cyan: 0x66d9ef,
            blue: 0x66d9ef,
            ..builtin::TOKYO_NIGHT
        };
        let theme = Theme::from_base("dupes", &base);
        assert_eq!(theme.palette.len(), 6);
        for name in builtin_names() {
            let palette = builtin(name).unwrap().palette;
            for (index, color) in palette.iter().enumerate() {
                assert!(
                    !palette[..index].contains(color),
                    "{name} repeats {color:?}"
                );
            }
        }
    }

    #[test]
    fn threshold_color_replaces_only_reset() {
        let theme = builtin("tokyo-night").unwrap();
        assert_eq!(theme.threshold_color(Color::Reset), theme.threshold_default);
        assert_eq!(theme.threshold_color(Color::Red), Color::Red);
    }
}
