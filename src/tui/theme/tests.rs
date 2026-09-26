use super::*;
use std::fs;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mfp-theme-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn every_preset_name_resolves_and_is_distinct() {
    let all: Vec<Theme> = THEME_NAMES
        .iter()
        .map(|n| Theme::preset(n).unwrap_or_else(|| panic!("missing preset {n}")))
        .collect();
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b);
        }
    }
    assert_eq!(THEME_NAMES[0], "default");
    assert_eq!(Theme::preset("nope"), None);
    assert_eq!(Theme::preset(""), None);
    // Resolving by name never touches the disk for presets.
    for n in THEME_NAMES {
        assert_eq!(
            Theme::resolve_in(n, None),
            (Theme::preset(n).unwrap(), None)
        );
    }
}

#[test]
fn default_preset_is_the_original_terminal_native_look() {
    let t = Theme::default();
    assert_eq!(t, Theme::default_preset());
    assert_eq!(t.background, Color::Reset);
    assert_eq!(t.foreground, Color::Reset);
    assert_eq!(t.base_style(), None);
    // Same styles the pre-theme code hard-coded.
    assert_eq!(t.border(), Style::default().add_modifier(Modifier::DIM));
    assert_eq!(
        t.title(),
        Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD)
    );
    assert_eq!(t.primary(), Style::default().add_modifier(Modifier::BOLD));
    assert_eq!(t.dim(), Style::default().add_modifier(Modifier::DIM));
    assert_eq!(t.accent(), Style::default().fg(Color::Blue));
    assert_eq!(
        t.favorite(),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    );
    assert_eq!(
        t.error(),
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    );
    assert_eq!(t.ok(), Style::default().fg(Color::Green));
    assert_eq!(t.status(), Style::default());
    assert_eq!(
        t.key(),
        Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD)
    );
    assert_eq!(t.help_text(), Style::default().add_modifier(Modifier::DIM));
    assert_eq!(
        t.selection(),
        Style::default().add_modifier(Modifier::REVERSED)
    );
    for frac in [0.0, 0.2, 0.5, 0.9, 1.0] {
        assert_eq!(t.level_style(frac), Style::default().fg(Color::Blue));
    }
    assert_eq!(t.viz_dim(), Style::default().add_modifier(Modifier::DIM));
}

#[test]
fn selection_uses_explicit_colors_unless_both_are_reset() {
    let mut t = Theme {
        selection_bg: Color::Rgb(1, 2, 3),
        ..Theme::default()
    };
    assert_eq!(t.selection(), Style::default().bg(Color::Rgb(1, 2, 3)));
    t.selection_fg = Color::White;
    assert_eq!(
        t.selection(),
        Style::default().fg(Color::White).bg(Color::Rgb(1, 2, 3))
    );
    assert!(!t.selection().add_modifier.contains(Modifier::REVERSED));
}

#[test]
fn base_style_paints_only_when_something_is_set() {
    let mut t = Theme {
        foreground: Color::White,
        ..Theme::default()
    };
    assert_eq!(t.base_style(), Some(Style::default().fg(Color::White)));
    t.background = Color::Black;
    assert_eq!(
        t.base_style(),
        Some(Style::default().fg(Color::White).bg(Color::Black))
    );
}

#[test]
fn gradient_interpolates_rgb_stops() {
    let (l, m, h) = (
        Color::Rgb(0, 0, 0),
        Color::Rgb(100, 200, 50),
        Color::Rgb(200, 0, 100),
    );
    assert_eq!(gradient(l, m, h, 0.0), l);
    assert_eq!(gradient(l, m, h, 0.25), Color::Rgb(50, 100, 25));
    assert_eq!(gradient(l, m, h, 0.5), m);
    assert_eq!(gradient(l, m, h, 0.75), Color::Rgb(150, 100, 75));
    assert_eq!(gradient(l, m, h, 1.0), h);
    // Out of range and non-finite inputs are clamped.
    assert_eq!(gradient(l, m, h, 7.0), h);
    assert_eq!(gradient(l, m, h, -1.0), l);
    assert_eq!(gradient(l, m, h, f32::NAN), l);
}

#[test]
fn gradient_steps_when_stops_are_not_all_rgb() {
    let (l, m, h) = (Color::Red, Color::Rgb(1, 2, 3), Color::Blue);
    assert_eq!(gradient(l, m, h, 0.0), l);
    assert_eq!(gradient(l, m, h, 0.33), l);
    assert_eq!(gradient(l, m, h, 0.34), m);
    assert_eq!(gradient(l, m, h, 0.66), m);
    assert_eq!(gradient(l, m, h, 0.67), h);
    assert_eq!(gradient(l, m, h, 1.0), h);
}

#[test]
fn theme_file_inherits_from_base_and_overrides_roles() {
    let t =
        Theme::parse_custom("base = \"dracula\"\naccent = \"#010203\"\nviz_high = \"light-red\"\n")
            .unwrap();
    let base = Theme::preset("dracula").unwrap();
    assert_eq!(t.accent, Color::Rgb(1, 2, 3));
    assert_eq!(t.viz_high, Color::LightRed);
    assert_eq!(
        Theme {
            accent: base.accent,
            viz_high: base.viz_high,
            ..t
        },
        base
    );
}

#[test]
fn theme_file_without_base_inherits_default() {
    let t = Theme::parse_custom("favorite = \"cyan\"").unwrap();
    assert_eq!(t.favorite, Color::Cyan);
    assert_eq!(
        Theme {
            favorite: Color::Yellow,
            ..t
        },
        Theme::default()
    );
    assert_eq!(Theme::parse_custom("").unwrap(), Theme::default());
}

#[test]
fn theme_file_accepts_named_hex_indexed_and_reset() {
    let t = Theme::parse_custom(
        "background = \"reset\"\nforeground = \"#268bd2\"\naccent = \"166\"\nmuted = \"light-blue\"\ndanger = \"Red\"\n",
    )
    .unwrap();
    assert_eq!(t.background, Color::Reset);
    assert_eq!(t.foreground, Color::Rgb(0x26, 0x8b, 0xd2));
    assert_eq!(t.accent, Color::Indexed(166));
    assert_eq!(t.muted, Color::LightBlue);
    assert_eq!(t.danger, Color::Red);
}

#[test]
fn theme_file_errors_name_the_problem() {
    let err = Theme::parse_custom("accent = \"not-a-color\"").unwrap_err();
    assert!(
        err.contains("accent") && err.contains("not-a-color"),
        "{err}"
    );
    assert!(
        Theme::parse_custom("acent = \"red\"").is_err(),
        "typo'd key"
    );
    assert!(Theme::parse_custom("accent = 5").is_err());
    assert!(Theme::parse_custom("[[").is_err());
    let err = Theme::parse_custom("base = \"mine\"").unwrap_err();
    assert!(err.contains("base"), "{err}");
}

#[test]
fn resolve_in_loads_custom_files_and_falls_back_to_default() {
    let dir = temp_dir("resolve");
    fs::write(
        dir.join("mine.toml"),
        "base = \"nord\"\nviz_wave = \"#ff0000\"\n",
    )
    .unwrap();
    fs::write(dir.join("bad.toml"), "viz_wave = \"nope\"\n").unwrap();
    fs::write(dir.join("chain.toml"), "base = \"mine\"\n").unwrap();

    let (t, msg) = Theme::resolve_in("mine", Some(&dir));
    assert_eq!(msg, None);
    assert_eq!(t.viz_wave, Color::Rgb(255, 0, 0));
    assert_eq!(t.accent, Theme::preset("nord").unwrap().accent);

    for name in ["bad", "chain", "missing", "../mine", "", ".."] {
        let (t, msg) = Theme::resolve_in(name, Some(&dir));
        assert_eq!(t, Theme::default(), "{name:?}");
        let msg = msg.unwrap_or_else(|| panic!("no message for {name:?}"));
        assert!(msg.contains("using 'default'"), "{msg}");
    }
    let (_, msg) = Theme::resolve_in("bad", Some(&dir));
    let msg = msg.unwrap();
    assert!(
        msg.contains("bad.toml") && msg.contains("viz_wave") && msg.contains("nope"),
        "{msg}"
    );

    let (t, msg) = Theme::resolve_in("mine", None);
    assert_eq!(t, Theme::default());
    assert!(msg.is_some());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_traversal_name_never_reads_outside_the_themes_dir() {
    let root = temp_dir("traversal");
    let themes = root.join("themes");
    fs::create_dir_all(&themes).unwrap();
    fs::write(root.join("outside.toml"), "accent = \"red\"\n").unwrap();
    let (t, msg) = Theme::resolve_in("../outside", Some(&themes));
    assert_eq!(t, Theme::default());
    assert!(msg.is_some_and(|m| m.contains("invalid theme name")));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn vu_style_maps_each_zone_to_its_role() {
    let t = Theme::preset("dracula").unwrap();
    assert_eq!(t.vu_style(VuZone::Ok), paint(t.viz_vu_ok));
    assert_eq!(t.vu_style(VuZone::Warn), paint(t.viz_vu_warn));
    assert_eq!(t.vu_style(VuZone::Clip), paint(t.viz_vu_clip));
}
