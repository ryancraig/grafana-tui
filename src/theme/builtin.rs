//! Built-in themes. Palette values come from each project's official sources.

use super::{Base, Theme, terminal};

pub(crate) const DEFAULT_THEME: &str = "tokyo-night";

enum Spec {
    Terminal,
    Base(Base),
}

struct Builtin {
    name: &'static str,
    spec: Spec,
}

const fn base(name: &'static str, base: Base) -> Builtin {
    Builtin {
        name,
        spec: Spec::Base(base),
    }
}

const BUILTINS: &[Builtin] = &[
    base("tokyo-night", TOKYO_NIGHT),
    base("tokyo-night-storm", TOKYO_NIGHT_STORM),
    base("tokyo-night-moon", TOKYO_NIGHT_MOON),
    base("tokyo-night-day", TOKYO_NIGHT_DAY),
    base("catppuccin-mocha", CATPPUCCIN_MOCHA),
    base("catppuccin-macchiato", CATPPUCCIN_MACCHIATO),
    base("catppuccin-frappe", CATPPUCCIN_FRAPPE),
    base("catppuccin-latte", CATPPUCCIN_LATTE),
    base("gruvbox-dark", GRUVBOX_DARK),
    base("gruvbox-dark-hard", GRUVBOX_DARK_HARD),
    base("gruvbox-dark-soft", GRUVBOX_DARK_SOFT),
    base("gruvbox-light", GRUVBOX_LIGHT),
    base("gruvbox-light-hard", GRUVBOX_LIGHT_HARD),
    base("gruvbox-light-soft", GRUVBOX_LIGHT_SOFT),
    base("dracula", DRACULA),
    base("monokai", MONOKAI),
    base("solarized-dark", SOLARIZED_DARK),
    base("solarized-light", SOLARIZED_LIGHT),
    Builtin {
        name: "terminal",
        spec: Spec::Terminal,
    },
];

pub(crate) const ALIASES: &[(&str, &str)] = &[
    ("default", DEFAULT_THEME),
    ("tokyo-night-night", "tokyo-night"),
    ("catppuccin", "catppuccin-mocha"),
    ("gruvbox", "gruvbox-dark"),
    ("gruvbox-dark-medium", "gruvbox-dark"),
    ("gruvbox-light-medium", "gruvbox-light"),
];

/// Canonical built-in theme names, in display order.
pub(crate) fn builtin_names() -> impl Iterator<Item = &'static str> {
    BUILTINS.iter().map(|builtin| builtin.name)
}

/// Looks up a built-in theme or alias, ignoring case.
pub(crate) fn builtin(name: &str) -> Option<Theme> {
    let name = name.to_ascii_lowercase();
    let name = ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map_or(name.as_str(), |(_, target)| target);
    let builtin = BUILTINS.iter().find(|builtin| builtin.name == name)?;
    Some(match &builtin.spec {
        Spec::Terminal => terminal(),
        Spec::Base(base) => Theme::from_base(builtin.name, base),
    })
}

// folke/tokyonight.nvim lua/tokyonight/colors/{storm,night,moon}.lua. Day comes
// from the generated extras/lua/tokyonight_day.lua, since day.lua inverts at runtime.
pub(super) const TOKYO_NIGHT: Base = Base {
    bg: 0x1a1b26,
    bg_alt: 0x16161e,
    ..TOKYO_NIGHT_STORM
};

const TOKYO_NIGHT_STORM: Base = Base {
    bg: 0x24283b,
    bg_alt: 0x1f2335,
    highlight: 0x292e42,
    subtle: 0x3b4261,
    border: 0x565f89,
    muted: 0x737aa2,
    fg: 0xc0caf5,
    title: 0x7aa2f7,
    focus: 0xff9e64,
    red: 0xf7768e,
    orange: 0xff9e64,
    yellow: 0xe0af68,
    green: 0x9ece6a,
    cyan: 0x7dcfff,
    blue: 0x7aa2f7,
    purple: 0xbb9af7,
};

const TOKYO_NIGHT_MOON: Base = Base {
    bg: 0x222436,
    bg_alt: 0x1e2030,
    highlight: 0x2f334d,
    subtle: 0x3b4261,
    border: 0x636da6,
    muted: 0x737aa2,
    fg: 0xc8d3f5,
    title: 0x82aaff,
    focus: 0xff966c,
    red: 0xff757f,
    orange: 0xff966c,
    yellow: 0xffc777,
    green: 0xc3e88d,
    cyan: 0x86e1fc,
    blue: 0x82aaff,
    purple: 0xc099ff,
};

const TOKYO_NIGHT_DAY: Base = Base {
    bg: 0xe1e2e7,
    bg_alt: 0xd0d5e3,
    highlight: 0xc4c8da,
    subtle: 0xa8aecb,
    border: 0x848cb5,
    muted: 0x68709a,
    fg: 0x3760bf,
    title: 0x2e7de9,
    focus: 0xb15c00,
    red: 0xf52a65,
    orange: 0xb15c00,
    yellow: 0x8c6c3e,
    green: 0x587539,
    cyan: 0x007197,
    blue: 0x2e7de9,
    purple: 0x9854f1,
};

// catppuccin/palette palette.json: base, mantle, surface0-2, overlay1 and text,
// with lavender for focus as the style guide suggests for active borders.
const CATPPUCCIN_MOCHA: Base = Base {
    bg: 0x1e1e2e,
    bg_alt: 0x181825,
    highlight: 0x313244,
    subtle: 0x45475a,
    border: 0x585b70,
    muted: 0x7f849c,
    fg: 0xcdd6f4,
    title: 0x89b4fa,
    focus: 0xb4befe,
    red: 0xf38ba8,
    orange: 0xfab387,
    yellow: 0xf9e2af,
    green: 0xa6e3a1,
    cyan: 0x94e2d5,
    blue: 0x89b4fa,
    purple: 0xcba6f7,
};

const CATPPUCCIN_MACCHIATO: Base = Base {
    bg: 0x24273a,
    bg_alt: 0x1e2030,
    highlight: 0x363a4f,
    subtle: 0x494d64,
    border: 0x5b6078,
    muted: 0x8087a2,
    fg: 0xcad3f5,
    title: 0x8aadf4,
    focus: 0xb7bdf8,
    red: 0xed8796,
    orange: 0xf5a97f,
    yellow: 0xeed49f,
    green: 0xa6da95,
    cyan: 0x8bd5ca,
    blue: 0x8aadf4,
    purple: 0xc6a0f6,
};

const CATPPUCCIN_FRAPPE: Base = Base {
    bg: 0x303446,
    bg_alt: 0x292c3c,
    highlight: 0x414559,
    subtle: 0x51576d,
    border: 0x626880,
    muted: 0x838ba7,
    fg: 0xc6d0f5,
    title: 0x8caaee,
    focus: 0xbabbf1,
    red: 0xe78284,
    orange: 0xef9f76,
    yellow: 0xe5c890,
    green: 0xa6d189,
    cyan: 0x81c8be,
    blue: 0x8caaee,
    purple: 0xca9ee6,
};

const CATPPUCCIN_LATTE: Base = Base {
    bg: 0xeff1f5,
    bg_alt: 0xe6e9ef,
    highlight: 0xccd0da,
    subtle: 0xbcc0cc,
    border: 0xacb0be,
    muted: 0x8c8fa1,
    fg: 0x4c4f69,
    title: 0x1e66f5,
    focus: 0x7287fd,
    red: 0xd20f39,
    orange: 0xfe640b,
    yellow: 0xdf8e1d,
    green: 0x40a02b,
    cyan: 0x179299,
    blue: 0x1e66f5,
    purple: 0x8839ef,
};

// morhetz/gruvbox colors/gruvbox.vim: dark backgrounds use the bright accents,
// light backgrounds the faded ones.
const GRUVBOX_DARK: Base = Base {
    bg: 0x282828,
    bg_alt: 0x3c3836,
    highlight: 0x504945,
    subtle: 0x504945,
    border: 0x7c6f64,
    muted: 0x928374,
    fg: 0xebdbb2,
    title: 0xfabd2f,
    focus: 0xfe8019,
    red: 0xfb4934,
    orange: 0xfe8019,
    yellow: 0xfabd2f,
    green: 0xb8bb26,
    cyan: 0x8ec07c,
    blue: 0x83a598,
    purple: 0xd3869b,
};

const GRUVBOX_DARK_HARD: Base = Base {
    bg: 0x1d2021,
    ..GRUVBOX_DARK
};

const GRUVBOX_DARK_SOFT: Base = Base {
    bg: 0x32302f,
    ..GRUVBOX_DARK
};

const GRUVBOX_LIGHT: Base = Base {
    bg: 0xfbf1c7,
    bg_alt: 0xebdbb2,
    highlight: 0xd5c4a1,
    subtle: 0xd5c4a1,
    border: 0xa89984,
    muted: 0x928374,
    fg: 0x3c3836,
    title: 0xb57614,
    focus: 0xaf3a03,
    red: 0x9d0006,
    orange: 0xaf3a03,
    yellow: 0xb57614,
    green: 0x79740e,
    cyan: 0x427b58,
    blue: 0x076678,
    purple: 0x8f3f71,
};

const GRUVBOX_LIGHT_HARD: Base = Base {
    bg: 0xf9f5d7,
    ..GRUVBOX_LIGHT
};

const GRUVBOX_LIGHT_SOFT: Base = Base {
    bg: 0xf2e5bc,
    ..GRUVBOX_LIGHT
};

// draculatheme.com/contribute: no blue, so purple and pink stand in.
const DRACULA: Base = Base {
    bg: 0x282a36,
    bg_alt: 0x21222c,
    highlight: 0x44475a,
    subtle: 0x44475a,
    border: 0x6272a4,
    muted: 0x6272a4,
    fg: 0xf8f8f2,
    title: 0xbd93f9,
    focus: 0xff79c6,
    red: 0xff5555,
    orange: 0xffb86c,
    yellow: 0xf1fa8c,
    green: 0x50fa7b,
    cyan: 0x8be9fd,
    blue: 0xbd93f9,
    purple: 0xff79c6,
};

const MONOKAI: Base = Base {
    bg: 0x272822,
    bg_alt: 0x1e1f1c,
    highlight: 0x3e3d32,
    subtle: 0x3e3d32,
    border: 0x75715e,
    muted: 0x75715e,
    fg: 0xf8f8f2,
    title: 0x66d9ef,
    focus: 0xfd971f,
    red: 0xf92672,
    orange: 0xfd971f,
    yellow: 0xe6db74,
    green: 0xa6e22e,
    cyan: 0xa1efe4,
    blue: 0x66d9ef,
    purple: 0xae81ff,
};

// ethanschoonover.com/solarized
const SOLARIZED_ACCENTS: Base = Base {
    bg: 0x002b36,
    bg_alt: 0x073642,
    highlight: 0x073642,
    subtle: 0x073642,
    border: 0x586e75,
    muted: 0x586e75,
    fg: 0x839496,
    title: 0x268bd2,
    focus: 0xb58900,
    red: 0xdc322f,
    orange: 0xcb4b16,
    yellow: 0xb58900,
    green: 0x859900,
    cyan: 0x2aa198,
    blue: 0x268bd2,
    purple: 0x6c71c4,
};

const SOLARIZED_DARK: Base = SOLARIZED_ACCENTS;

const SOLARIZED_LIGHT: Base = Base {
    bg: 0xfdf6e3,
    bg_alt: 0xeee8d5,
    highlight: 0xeee8d5,
    subtle: 0xeee8d5,
    border: 0x93a1a1,
    muted: 0x93a1a1,
    fg: 0x657b83,
    ..SOLARIZED_ACCENTS
};
