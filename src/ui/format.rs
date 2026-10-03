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

use ratatui::style::Color;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct DisplayFormat {
    pub(crate) unit: Option<String>,
    pub(crate) decimals: Option<usize>,
    pub(crate) no_value: Option<String>,
}

impl DisplayFormat {
    pub(crate) fn format_value(&self, value: Option<f64>) -> String {
        value
            .map(|value| self.format_number(value))
            .unwrap_or_else(|| self.no_value.clone().unwrap_or_else(|| "-".to_string()))
    }

    /// What a panel shows when its queries returned nothing: Grafana's
    /// `noValue`, or `No data`.
    pub(crate) fn no_data_text(&self) -> &str {
        self.no_value.as_deref().unwrap_or("No data")
    }

    pub(crate) fn format_number(&self, value: f64) -> String {
        // Grafana's common units; unknown units keep the compact SI format.
        let decimals = self.decimals;
        match self.unit_key().as_deref() {
            Some("bytes") => format_scaled(value, &BYTES_IEC, KIB, decimals),
            Some("decbytes") => format_scaled(value, &BYTES_SI, 1000.0, decimals),
            Some("kbytes") => format_scaled(value * KIB, &BYTES_IEC, KIB, decimals),
            Some("deckbytes") => format_scaled(value * 1e3, &BYTES_SI, 1000.0, decimals),
            Some("mbytes") => format_scaled(value * KIB * KIB, &BYTES_IEC, KIB, decimals),
            Some("decmbytes") => format_scaled(value * 1e6, &BYTES_SI, 1000.0, decimals),
            Some("gbytes") => format_scaled(value * KIB * KIB * KIB, &BYTES_IEC, KIB, decimals),
            Some("decgbytes") => format_scaled(value * 1e9, &BYTES_SI, 1000.0, decimals),
            Some("bits") => format_scaled(value, &BITS_IEC, KIB, decimals),
            Some("decbits") => format_scaled(value, &BITS_SI, 1000.0, decimals),
            Some("bps") => format_scaled(value, &BIT_RATE_SI, 1000.0, decimals),
            Some("Bps" | "bytes/sec" | "bytes/s") => {
                format_scaled(value, &BYTE_RATE_SI, 1000.0, decimals)
            }
            Some("binbps") => format_scaled(value, &BIT_RATE_IEC, KIB, decimals),
            Some("binBps") => format_scaled(value, &BYTE_RATE_IEC, KIB, decimals),
            Some("ns") => format_duration_seconds(value / 1e9, decimals),
            Some("µs" | "us") => format_duration_seconds(value / 1e6, decimals),
            Some("ms") => format_duration_seconds(value / 1e3, decimals),
            Some("s" | "seconds") => format_duration_seconds(value, decimals),
            Some("m") => format_duration_seconds(value * 60.0, decimals),
            Some("h") => format_duration_seconds(value * 3_600.0, decimals),
            Some("d") => format_duration_seconds(value * 86_400.0, decimals),
            Some("percent") => format_suffix(value, decimals, "%"),
            // Grafana's percentunit stores ratios as 0.0-1.0, so scale to the
            // user-facing percent text dashboards expect.
            Some("percentunit") => format_suffix(value * 100.0, decimals, "%"),
            Some("ops") => format_rate(value, decimals, " ops/s"),
            Some("reqps" | "rps") => format_rate(value, decimals, " req/s"),
            Some("short" | "none") | None => format_si_with_decimals(value, decimals),
            Some(_) => format_si_with_decimals(value, decimals),
        }
    }

    /// The unit, ignoring case except where Grafana units differ only by
    /// case: `bps` is bits and `Bps` bytes per second.
    fn unit_key(&self) -> Option<String> {
        let unit = self
            .unit
            .as_deref()
            .map(str::trim)
            .filter(|unit| !unit.is_empty())?;
        Some(match unit {
            "Bps" | "binBps" | "binbps" => unit.to_string(),
            _ => unit.to_lowercase(),
        })
    }
}

/// Maps a normalized value (0.0-1.0) to the theme's low, mid or high heatmap band.
pub(crate) fn value_to_heatmap_color(normalized: f64, bands: [Color; 3]) -> Color {
    if normalized < 0.33 {
        bands[0]
    } else if normalized < 0.66 {
        bands[1]
    } else {
        bands[2]
    }
}

fn format_si_with_decimals(val: f64, decimals: Option<usize>) -> String {
    let abs = val.abs();
    let decimals = decimals.unwrap_or(2);
    if abs >= 1e9 {
        format!("{:.*}G", decimals, val / 1e9)
    } else if abs >= 1e6 {
        format!("{:.*}M", decimals, val / 1e6)
    } else if abs >= 1e3 {
        format!("{:.*}k", decimals, val / 1e3)
    } else {
        format!("{:.*}", decimals, val)
    }
}

fn format_suffix(value: f64, decimals: Option<usize>, suffix: &str) -> String {
    format!("{:.*}{suffix}", decimals.unwrap_or(2), value)
}

fn format_rate(value: f64, decimals: Option<usize>, suffix: &str) -> String {
    format!("{}{suffix}", format_si_with_decimals(value, decimals))
}

const KIB: f64 = 1024.0;
const BYTES_IEC: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
const BYTES_SI: [&str; 6] = ["B", "kB", "MB", "GB", "TB", "PB"];
const BITS_IEC: [&str; 6] = ["b", "Kib", "Mib", "Gib", "Tib", "Pib"];
const BITS_SI: [&str; 6] = ["b", "kb", "Mb", "Gb", "Tb", "Pb"];
const BIT_RATE_SI: [&str; 6] = ["bps", "kbps", "Mbps", "Gbps", "Tbps", "Pbps"];
const BYTE_RATE_SI: [&str; 6] = ["B/s", "kB/s", "MB/s", "GB/s", "TB/s", "PB/s"];
const BIT_RATE_IEC: [&str; 6] = ["b/s", "Kib/s", "Mib/s", "Gib/s", "Tib/s", "Pib/s"];
const BYTE_RATE_IEC: [&str; 6] = ["B/s", "KiB/s", "MiB/s", "GiB/s", "TiB/s", "PiB/s"];

/// Scales `value` by `base` until it fits the largest suffix it reaches.
fn format_scaled(value: f64, suffixes: &[&str], base: f64, decimals: Option<usize>) -> String {
    let mut scaled = value;
    let mut suffix_index = 0;

    while scaled.abs() >= base && suffix_index + 1 < suffixes.len() {
        scaled /= base;
        suffix_index += 1;
    }

    format!(
        "{:.*}{}",
        decimals.unwrap_or(2),
        scaled,
        suffixes[suffix_index]
    )
}

/// Formats seconds in the largest unit that keeps the value at least 1, from
/// microseconds to years, as Grafana's time units do.
fn format_duration_seconds(value: f64, decimals: Option<usize>) -> String {
    const UNITS: [(f64, &str); 8] = [
        (31_536_000.0, "y"),
        (604_800.0, "w"),
        (86_400.0, "d"),
        (3_600.0, "h"),
        (60.0, "m"),
        (1.0, "s"),
        (1e-3, "ms"),
        (1e-6, "µs"),
    ];
    let abs = value.abs();
    if abs == 0.0 {
        return format_suffix(0.0, decimals, "s");
    }
    let (size, suffix) = UNITS
        .into_iter()
        .find(|(size, _)| abs >= *size)
        .unwrap_or((1e-9, "ns"));
    format_suffix(value / size, decimals, suffix)
}

pub(crate) fn format_time(ts: f64) -> String {
    use chrono::TimeZone;
    if let Some(dt) = chrono::Utc.timestamp_opt(ts as i64, 0).single() {
        dt.format("%H:%M:%S").to_string()
    } else {
        format!("{}", ts)
    }
}

pub(crate) fn format_axis_time(ts: f64, range_secs: f64) -> String {
    use chrono::{TimeZone, Timelike};

    const DAY: f64 = 24.0 * 60.0 * 60.0;
    if range_secs < DAY {
        return format_time(ts);
    }

    let Some(dt) = chrono::Utc.timestamp_opt(ts as i64, 0).single() else {
        return format!("{}", ts);
    };

    if range_secs < 7.0 * DAY {
        if dt.hour() == 0 && dt.minute() == 0 && dt.second() == 0 {
            dt.format("%b %d").to_string()
        } else {
            dt.format("%b %d %Hh").to_string()
        }
    } else if range_secs < 90.0 * DAY {
        dt.format("%b %d").to_string()
    } else if range_secs < 730.0 * DAY {
        dt.format("%Y-%m").to_string()
    } else {
        dt.format("%Y").to_string()
    }
}

/// Generate a color from a string using hash-based approach.
/// Uses HSL color space to ensure visually distinct, vibrant colors.
pub(crate) fn get_hash_color(name: &str) -> Color {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();

    // Use HSL color space for better color distribution
    // Hue: use the hash to get different hues (0-360 degrees)
    let hue = (hash % 360) as f32;

    // Saturation: keep high for vibrant colors (60-90%)
    let saturation = 60.0 + ((hash >> 8) % 30) as f32;

    // Lightness: keep in a range that's visible on both light and dark backgrounds (45-65%)
    let lightness = 45.0 + ((hash >> 16) % 20) as f32;

    hsl_to_rgb(hue, saturation, lightness)
}

/// Convert HSL to RGB color for ratatui.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Color {
    let s = s / 100.0;
    let l = l / 100.0;

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    Color::Rgb(
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_format_uses_si_for_missing_and_unknown_units() {
        let default = DisplayFormat::default();
        assert_eq!(default.format_number(1_500.0), "1.50k");

        let unknown = DisplayFormat {
            unit: Some("widgets".to_string()),
            decimals: None,
            no_value: None,
        };
        assert_eq!(unknown.format_number(1_500.0), "1.50k");
    }

    #[test]
    fn test_display_format_scales_bytes_and_bits() {
        let bytes = DisplayFormat {
            unit: Some("bytes".to_string()),
            decimals: None,
            no_value: None,
        };
        assert_eq!(bytes.format_number(1_536.0), "1.50KiB");

        let bits = DisplayFormat {
            unit: Some("bits".to_string()),
            decimals: Some(1),
            no_value: None,
        };
        assert_eq!(bits.format_number(1_536.0), "1.5Kib");
    }

    fn unit(unit: &str) -> DisplayFormat {
        DisplayFormat {
            unit: Some(unit.to_string()),
            decimals: Some(1),
            no_value: None,
        }
    }

    #[test]
    fn data_units_follow_grafana_si_and_iec_prefixes() {
        assert_eq!(unit("bytes").format_number(1_048_576.0), "1.0MiB");
        assert_eq!(unit("decbytes").format_number(1_500_000.0), "1.5MB");
        assert_eq!(unit("decbytes").format_number(1_500.0), "1.5kB");
        assert_eq!(unit("decbits").format_number(1_500.0), "1.5kb");
        assert_eq!(unit("kbytes").format_number(2_048.0), "2.0MiB");
        assert_eq!(unit("decgbytes").format_number(1.5), "1.5GB");
        assert_eq!(unit("bytes").format_number(-2_048.0), "-2.0KiB");
    }

    #[test]
    fn rate_units_tell_bits_from_bytes_by_case() {
        assert_eq!(unit("bps").format_number(1_500_000.0), "1.5Mbps");
        assert_eq!(unit("Bps").format_number(1_500_000.0), "1.5MB/s");
        assert_eq!(unit("binbps").format_number(2_048.0), "2.0Kib/s");
        assert_eq!(unit("binBps").format_number(2_048.0), "2.0KiB/s");
        // Units that differ only by case elsewhere are matched case-insensitively.
        assert_eq!(unit("Bytes").format_number(2_048.0), "2.0KiB");
        assert_eq!(unit("bytes/sec").format_number(1_500.0), "1.5kB/s");
    }

    #[test]
    fn time_units_scale_up_and_down() {
        assert_eq!(unit("ms").format_number(250.0), "250.0ms");
        assert_eq!(unit("ms").format_number(1_500.0), "1.5s");
        assert_eq!(unit("ms").format_number(90_000.0), "1.5m");
        assert_eq!(unit("s").format_number(0.25), "250.0ms");
        assert_eq!(unit("s").format_number(0.000_5), "500.0µs");
        assert_eq!(unit("s").format_number(7_200.0), "2.0h");
        assert_eq!(unit("s").format_number(1_209_600.0), "2.0w");
        assert_eq!(unit("s").format_number(0.0), "0.0s");
        assert_eq!(unit("µs").format_number(1_500.0), "1.5ms");
        assert_eq!(unit("ns").format_number(1_500.0), "1.5µs");
        assert_eq!(unit("m").format_number(90.0), "1.5h");
        assert_eq!(unit("d").format_number(14.0), "2.0w");
    }

    #[test]
    fn test_display_format_percent_and_percentunit() {
        let percent = DisplayFormat {
            unit: Some("percent".to_string()),
            decimals: Some(1),
            no_value: None,
        };
        assert_eq!(percent.format_number(42.42), "42.4%");

        let percent_unit = DisplayFormat {
            unit: Some("percentunit".to_string()),
            decimals: Some(0),
            no_value: None,
        };
        assert_eq!(percent_unit.format_number(0.4242), "42%");
    }

    #[test]
    fn test_display_format_decimals_no_value_and_rates() {
        let rate = DisplayFormat {
            unit: Some("reqps".to_string()),
            decimals: Some(0),
            no_value: Some("n/a".to_string()),
        };
        assert_eq!(rate.format_number(1234.56), "1k req/s");
        assert_eq!(rate.format_value(None), "n/a");
        assert_eq!(rate.no_data_text(), "n/a");

        let default = DisplayFormat::default();
        assert_eq!(default.format_value(None), "-");
        assert_eq!(default.no_data_text(), "No data");
    }

    #[test]
    fn test_format_axis_time_uses_time_for_short_ranges() {
        use chrono::TimeZone;

        let ts = chrono::Utc
            .with_ymd_and_hms(2026, 4, 30, 12, 34, 56)
            .single()
            .unwrap()
            .timestamp() as f64;

        assert_eq!(format_axis_time(ts, 60.0 * 60.0), "12:34:56");
    }

    #[test]
    fn test_format_axis_time_uses_date_for_multi_day_midnight_ticks() {
        use chrono::TimeZone;

        let ts = chrono::Utc
            .with_ymd_and_hms(2026, 4, 30, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp() as f64;

        assert_eq!(format_axis_time(ts, 2.0 * 24.0 * 60.0 * 60.0), "Apr 30");
    }

    #[test]
    fn test_format_axis_time_keeps_hour_for_multi_day_non_midnight_ticks() {
        use chrono::TimeZone;

        let ts = chrono::Utc
            .with_ymd_and_hms(2026, 4, 30, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp() as f64;

        assert_eq!(format_axis_time(ts, 2.0 * 24.0 * 60.0 * 60.0), "Apr 30 12h");
    }

    #[test]
    fn test_format_axis_time_scales_to_wider_ranges() {
        use chrono::TimeZone;

        let ts = chrono::Utc
            .with_ymd_and_hms(2026, 4, 30, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp() as f64;
        let day = 24.0 * 60.0 * 60.0;

        assert_eq!(format_axis_time(ts, 14.0 * day), "Apr 30");
        assert_eq!(format_axis_time(ts, 120.0 * day), "2026-04");
        assert_eq!(format_axis_time(ts, 800.0 * day), "2026");
    }
}
