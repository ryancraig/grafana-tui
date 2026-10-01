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

use ratatui::style::Color;

pub(crate) use builtin::builtin;
#[cfg(test)]
use builtin::builtin_names;

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
        terminal()
    }
}

impl Theme {
    pub(crate) fn from_str(name: &str) -> Self {
        builtin(name).unwrap_or_default()
    }

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
    fn unknown_names_fall_back_to_the_terminal_theme() {
        assert_eq!(Theme::from_str("nope").name, "terminal");
        assert_eq!(Theme::from_str("default").name, "terminal");
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
