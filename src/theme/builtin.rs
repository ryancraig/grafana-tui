//! Built-in themes. Palette values come from each project's official sources.

use super::{Base, Theme, terminal};

enum Spec {
    Terminal,
    Base(Base),
}

struct Builtin {
    name: &'static str,
    spec: Spec,
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "terminal",
        spec: Spec::Terminal,
    },
    Builtin {
        name: "tokyo-night",
        spec: Spec::Base(TOKYO_NIGHT),
    },
    Builtin {
        name: "catppuccin",
        spec: Spec::Base(CATPPUCCIN_MOCHA),
    },
    Builtin {
        name: "gruvbox",
        spec: Spec::Base(GRUVBOX_DARK),
    },
    Builtin {
        name: "dracula",
        spec: Spec::Base(DRACULA),
    },
    Builtin {
        name: "monokai",
        spec: Spec::Base(MONOKAI),
    },
    Builtin {
        name: "solarized-dark",
        spec: Spec::Base(SOLARIZED_DARK),
    },
    Builtin {
        name: "solarized-light",
        spec: Spec::Base(SOLARIZED_LIGHT),
    },
];

const ALIASES: &[(&str, &str)] = &[("default", "terminal")];

/// Canonical built-in theme names, in display order.
#[cfg(test)]
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

// folke/tokyonight.nvim lua/tokyonight/colors/{storm,night}.lua
pub(super) const TOKYO_NIGHT: Base = Base {
    bg: 0x1a1b26,
    bg_alt: 0x16161e,
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

// catppuccin/palette palette.json
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

// morhetz/gruvbox colors/gruvbox.vim: dark backgrounds use the bright accents.
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
