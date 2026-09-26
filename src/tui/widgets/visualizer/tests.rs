use super::*;
use ratatui::style::Modifier;

fn data_bands(bands: Vec<f32>) -> VisualizerData {
    VisualizerData {
        bands,
        ..Default::default()
    }
}

fn draw(style: VisualStyle, data: &VisualizerData, w: u16, h: u16) -> Buffer {
    let area = Rect::new(0, 0, w, h);
    let mut buf = Buffer::empty(area);
    render(style, data, &Theme::default(), area, &mut buf);
    buf
}

fn sym(buf: &Buffer, x: u16, y: u16) -> String {
    buf[(x, y)].symbol().to_string()
}

fn row(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| sym(buf, x, y)).collect()
}

fn grid(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| row(buf, y))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Eighths of a cell that are colored with the foreground (accent), taking
/// the reversed-video technique of Mirror into account.
fn fill(buf: &Buffer, x: u16, y: u16) -> usize {
    let cell = &buf[(x, y)];
    let n = match cell.symbol().chars().next() {
        Some(c) => BLOCKS.iter().position(|b| *b == c).map_or(0, |i| i + 1),
        None => 0,
    };
    if cell.modifier.contains(Modifier::REVERSED) {
        8 - n
    } else {
        n
    }
}

fn column_fill(buf: &Buffer, x: u16) -> usize {
    (0..buf.area.height).map(|y| fill(buf, x, y)).sum()
}

fn ramp_data(n: usize) -> Vec<f32> {
    (0..n).map(|i| i as f32 / (n - 1) as f32).collect()
}

fn is_braille(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| ('\u{2801}'..='\u{28FF}').contains(&c))
}

fn sine(n: usize, cycles: f32) -> Vec<f32> {
    (0..n)
        .map(|i| (i as f32 / n as f32 * cycles * std::f32::consts::TAU).sin())
        .collect()
}

#[test]
fn bars_silence_is_blank() {
    let buf = draw(VisualStyle::Bars, &data_bands(vec![0.0; 10]), 20, 5);
    assert!(grid(&buf).chars().all(|c| c == ' ' || c == '\n'));
    let empty = draw(VisualStyle::Bars, &VisualizerData::default(), 20, 5);
    assert!(grid(&empty).chars().all(|c| c == ' ' || c == '\n'));
}

#[test]
fn bars_full_scale_fills_height_with_gaps() {
    let buf = draw(VisualStyle::Bars, &data_bands(vec![1.0; 5]), 9, 4);
    for y in 0..4 {
        assert_eq!(row(&buf, y), "█ █ █ █ █");
    }
    // Narrow areas drop the gap.
    let narrow = draw(VisualStyle::Bars, &data_bands(vec![1.0; 5]), 5, 2);
    assert_eq!(row(&narrow, 0), "█████");
}

#[test]
fn bars_use_eighth_steps() {
    // Height 2 gives 16 steps: 0.5 -> 8 steps = one full cell.
    let buf = draw(VisualStyle::Bars, &data_bands(vec![0.5]), 1, 2);
    assert_eq!(
        (sym(&buf, 0, 1).as_str(), sym(&buf, 0, 0).as_str()),
        ("█", " ")
    );
    // 3/16 -> 3 steps of the lowest cell.
    let buf = draw(VisualStyle::Bars, &data_bands(vec![3.0 / 16.0]), 1, 2);
    assert_eq!(sym(&buf, 0, 1), "▃");
    for (i, b) in BLOCKS.iter().enumerate() {
        let buf = draw(
            VisualStyle::Bars,
            &data_bands(vec![(i + 1) as f32 / 8.0]),
            1,
            1,
        );
        assert_eq!(sym(&buf, 0, 0), b.to_string());
    }
}

#[test]
fn bars_ramp_is_monotonic() {
    let buf = draw(VisualStyle::Bars, &data_bands(ramp_data(20)), 20, 6);
    let heights: Vec<usize> = (0..10).map(|i| column_fill(&buf, i * 2)).collect();
    assert!(heights.windows(2).all(|w| w[0] <= w[1]), "{heights:?}");
    assert!(heights[9] > heights[0]);
}

#[test]
fn bars_spike_survives_downsampling() {
    let mut bands = vec![0.0; 64];
    bands[37] = 1.0;
    let buf = draw(VisualStyle::Bars, &data_bands(bands), 8, 3);
    let full = (0..8).filter(|x| column_fill(&buf, *x) == 24).count();
    assert_eq!(full, 1, "{}", grid(&buf));
}

#[test]
fn bars_alternating_levels() {
    let bands: Vec<f32> = (0..8).map(|i| if i % 2 == 0 { 1.0 } else { 0.0 }).collect();
    // Width 15 gives 8 gapped bars, one per band.
    let buf = draw(VisualStyle::Bars, &data_bands(bands), 15, 2);
    assert_eq!(row(&buf, 0), "█   █   █   █  ");
}

#[test]
fn mirror_is_vertically_symmetric() {
    for h in [2u16, 4, 5, 8, 9] {
        let buf = draw(VisualStyle::Mirror, &data_bands(ramp_data(16)), 16, h);
        let half = h / 2;
        let odd = h % 2;
        for x in 0..16 {
            let top: usize = (0..half).map(|y| fill(&buf, x, y)).sum();
            let bottom: usize = (half + odd..h).map(|y| fill(&buf, x, y)).sum();
            assert_eq!(top, bottom, "h={h} x={x}\n{}", grid(&buf));
        }
    }
}

#[test]
fn mirror_full_scale_and_silence() {
    let buf = draw(VisualStyle::Mirror, &data_bands(vec![1.0; 4]), 4, 4);
    assert!(grid(&buf).chars().all(|c| c == '█' || c == '\n'));
    let buf = draw(VisualStyle::Mirror, &data_bands(vec![0.0; 4]), 4, 5);
    assert!(grid(&buf).chars().all(|c| c == ' ' || c == '\n'));
}

#[test]
fn mirror_lower_half_uses_reversed_complement() {
    // Height 2 (half = 1), level 3/8: up cell ▃, down cell ▅ reversed.
    let buf = draw(VisualStyle::Mirror, &data_bands(vec![3.0 / 8.0]), 1, 2);
    assert_eq!(sym(&buf, 0, 0), "▃");
    assert_eq!(sym(&buf, 0, 1), "▅");
    assert!(buf[(0, 1)].modifier.contains(Modifier::REVERSED));
    assert!(!buf[(0, 0)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn wave_sine_draws_braille_across_the_width() {
    let data = VisualizerData {
        waveform: sine(400, 3.0),
        ..Default::default()
    };
    let buf = draw(VisualStyle::Wave, &data, 30, 6);
    for x in 0..30 {
        assert!(
            (0..6).any(|y| is_braille(&sym(&buf, x, y))),
            "column {x}\n{}",
            grid(&buf)
        );
    }
}

#[test]
fn wave_silence_is_a_flat_center_line() {
    for waveform in [vec![], vec![0.0; 300]] {
        let data = VisualizerData {
            waveform,
            ..Default::default()
        };
        let buf = draw(VisualStyle::Wave, &data, 20, 5);
        let rows: Vec<u16> = (0..5)
            .filter(|y| row(&buf, *y).chars().any(|c| c != ' '))
            .collect();
        assert_eq!(rows.len(), 1, "{}", grid(&buf));
        assert!((1..=3).contains(&rows[0]));
        assert!(row(&buf, rows[0]).chars().all(|c| c != ' '));
    }
}

#[test]
fn wave_full_scale_square_connects_top_and_bottom() {
    let mut w = vec![1.0; 20];
    w.extend(vec![-1.0; 20]);
    let data = VisualizerData {
        waveform: w,
        ..Default::default()
    };
    let buf = draw(VisualStyle::Wave, &data, 10, 3);
    assert!(row(&buf, 0).chars().any(|c| c != ' '));
    assert!(row(&buf, 2).chars().any(|c| c != ' '));
    // The vertical edge is drawn, so the middle row has dots too.
    assert!(row(&buf, 1).chars().any(|c| c != ' '));
}

#[test]
fn vu_fills_proportionally() {
    let data = VisualizerData {
        level_l: 0.5,
        level_r: 1.0,
        ..Default::default()
    };
    // Width 22 = 2 label cells + 20 segments.
    let buf = draw(VisualStyle::Vu, &data, 22, 2);
    let count = |y, c| row(&buf, y).chars().filter(|x| *x == c).count();
    assert!(row(&buf, 0).starts_with("L "));
    assert!(row(&buf, 1).starts_with("R "));
    assert_eq!((count(0, '▮'), count(0, '▯')), (10, 10));
    assert_eq!((count(1, '▮'), count(1, '▯')), (20, 0));
    let silent = draw(VisualStyle::Vu, &VisualizerData::default(), 22, 2);
    assert_eq!(row(&silent, 0).chars().filter(|c| *c == '▮').count(), 0);
}

#[test]
fn vu_layout_by_height_and_peak_tick() {
    let data = VisualizerData {
        level_l: 0.2,
        level_r: 0.2,
        hold_l: 0.8,
        ..Default::default()
    };
    let buf = draw(VisualStyle::Vu, &data, 22, 5);
    // Height 5 -> 3 rows used (L, blank, R) centered at rows 1..=3.
    assert_eq!(row(&buf, 0).trim(), "");
    assert!(row(&buf, 1).starts_with('L'));
    assert_eq!(row(&buf, 2).trim(), "");
    assert!(row(&buf, 3).starts_with('R'));
    // Tick at position 16 of 20 (cell 2 + 15), bold.
    assert_eq!(sym(&buf, 17, 1), "▮");
    assert!(buf[(17, 1)].modifier.contains(Modifier::BOLD));
    assert_eq!(sym(&buf, 16, 1), "▯");
    // Single row shows the louder channel as "M".
    let one = draw(VisualStyle::Vu, &data, 22, 1);
    assert!(row(&one, 0).starts_with('M'));
}

#[test]
fn dots_show_peak_marker_at_the_right_height() {
    let data = VisualizerData {
        bands: vec![0.5],
        peaks: vec![0.8],
        ..Default::default()
    };
    let buf = draw(VisualStyle::Dots, &data, 1, 10);
    // Peak 0.8 of 10 rows -> 8th row from the bottom -> y = 2.
    assert_eq!(sym(&buf, 0, 2), "▪");
    // Level 0.5 -> 5 dots (y 5..=9), no dots above the level besides the peak.
    for y in 5..10 {
        assert_eq!(sym(&buf, 0, y), "·", "y={y}");
    }
    assert_eq!(sym(&buf, 0, 3), " ");
    assert_eq!(sym(&buf, 0, 4), " ");
}

#[test]
fn dots_fall_back_to_bands_without_peaks() {
    let buf = draw(VisualStyle::Dots, &data_bands(vec![0.5]), 1, 10);
    assert_eq!(sym(&buf, 0, 5), "▪");
    assert_eq!(sym(&buf, 0, 9), "·");
}

#[test]
fn area_full_scale_ramp_and_silence() {
    let buf = draw(VisualStyle::Area, &data_bands(vec![1.0; 3]), 12, 3);
    assert!(grid(&buf).chars().all(|c| c == '█' || c == '\n'));
    let buf = draw(VisualStyle::Area, &data_bands(vec![0.0; 3]), 12, 3);
    assert!(grid(&buf).chars().all(|c| c == ' ' || c == '\n'));
    // Fewer bands than columns are interpolated: every column is used and the
    // silhouette never decreases.
    let buf = draw(VisualStyle::Area, &data_bands(vec![0.0, 1.0]), 16, 4);
    let heights: Vec<usize> = (0..16).map(|x| column_fill(&buf, x)).collect();
    assert!(heights.windows(2).all(|w| w[0] <= w[1]), "{heights:?}");
    assert_eq!(heights[15], 32);
    assert!(heights[7] > 0 && heights[7] < 32);
}

#[test]
fn area_edge_uses_partial_block_over_dim_fill() {
    let buf = draw(VisualStyle::Area, &data_bands(vec![0.75]), 1, 2);
    // 12 of 16 steps: full block below, half block on top.
    assert_eq!(sym(&buf, 0, 1), "█");
    assert_eq!(sym(&buf, 0, 0), "▄");
    assert!(buf[(0, 1)].modifier.contains(Modifier::DIM));
    assert!(!buf[(0, 0)].modifier.contains(Modifier::DIM));
}

#[test]
fn every_style_survives_degenerate_areas_and_data() {
    let bad = VisualizerData {
        bands: vec![f32::NAN, f32::INFINITY, -f32::INFINITY, 7.0, -3.0, 0.4],
        peaks: vec![f32::NAN, 9.0],
        waveform: vec![f32::NAN, f32::INFINITY, -50.0, 50.0, 0.3],
        level_l: f32::NAN,
        level_r: 12.0,
        hold_l: f32::INFINITY,
        hold_r: -1.0,
    };
    let big = VisualizerData {
        bands: ramp_data(5000),
        peaks: ramp_data(5000),
        waveform: sine(100_000, 50.0),
        level_l: 0.5,
        level_r: 0.5,
        hold_l: 0.9,
        hold_r: 0.9,
    };
    let datas = [VisualizerData::default(), bad, big];
    let rects = [
        Rect::new(0, 0, 0, 0),
        Rect::new(0, 0, 1, 1),
        Rect::new(0, 0, 1, 20),
        Rect::new(0, 0, 20, 1),
        Rect::new(0, 0, 2, 2),
        Rect::new(0, 0, 3, 3),
        Rect::new(0, 0, 500, 200),
        // Extends past the buffer and starts inside it.
        Rect::new(5, 5, 400, 400),
        // Entirely outside the buffer.
        Rect::new(900, 900, 10, 10),
    ];
    for style in VisualStyle::ALL {
        for data in &datas {
            for rect in rects {
                let mut buf = Buffer::empty(Rect::new(0, 0, 500, 200));
                render(style, data, &Theme::default(), rect, &mut buf);
                Visualizer {
                    style,
                    data,
                    theme: &Theme::default(),
                }
                .render(rect, &mut buf);
            }
            // Zero-sized buffer.
            let mut buf = Buffer::empty(Rect::new(0, 0, 0, 0));
            render(
                style,
                data,
                &Theme::default(),
                Rect::new(0, 0, 10, 10),
                &mut buf,
            );
        }
    }
}

#[test]
fn non_finite_input_draws_like_silence() {
    let nan = VisualizerData {
        bands: vec![f32::NAN; 8],
        peaks: vec![f32::NAN; 8],
        waveform: vec![f32::NAN; 40],
        level_l: f32::NAN,
        level_r: f32::NEG_INFINITY,
        hold_l: f32::NAN,
        hold_r: f32::NAN,
    };
    let silent = VisualizerData {
        bands: vec![0.0; 8],
        peaks: vec![0.0; 8],
        waveform: vec![0.0; 40],
        ..Default::default()
    };
    for style in VisualStyle::ALL {
        assert_eq!(
            grid(&draw(style, &nan, 24, 6)),
            grid(&draw(style, &silent, 24, 6)),
            "{style:?}"
        );
    }
}

#[test]
fn drawing_stays_inside_the_area() {
    let data = data_bands(vec![1.0; 16]);
    for style in VisualStyle::ALL {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 10));
        render(
            style,
            &data,
            &Theme::default(),
            Rect::new(4, 3, 8, 4),
            &mut buf,
        );
        for y in 0..10 {
            for x in 0..20 {
                let inside = (4..12).contains(&x) && (3..7).contains(&y);
                if !inside {
                    assert_eq!(sym(&buf, x, y), " ", "{style:?} ({x},{y})");
                }
            }
        }
    }
}

#[test]
fn rotation_cycles_through_all_styles() {
    let mut style = VisualStyle::default();
    assert_eq!(style, VisualStyle::Bars);
    let mut seen = Vec::new();
    for _ in 0..VisualStyle::ALL.len() {
        seen.push(style);
        style = style.next();
    }
    assert_eq!(style, VisualStyle::Bars);
    assert_eq!(seen, VisualStyle::ALL.to_vec());
    for s in VisualStyle::ALL {
        assert_eq!(s.next().prev(), s);
        assert_eq!(s.prev().next(), s);
    }
    assert_eq!(VisualStyle::Bars.prev(), VisualStyle::Vu);
}

#[test]
fn names_round_trip_and_ignore_case() {
    for s in VisualStyle::ALL {
        assert_eq!(VisualStyle::from_name(s.name()), Some(s));
        assert_eq!(VisualStyle::from_name(&s.name().to_uppercase()), Some(s));
        assert_eq!(s.name(), s.name().to_lowercase());
    }
    assert_eq!(
        VisualStyle::from_name(" Mirror "),
        Some(VisualStyle::Mirror)
    );
    assert_eq!(VisualStyle::from_name("nope"), None);
    assert_eq!(VisualStyle::from_name(""), None);
}

#[test]
fn bands_wanted_is_sane() {
    for style in VisualStyle::ALL {
        assert_eq!(bands_wanted(style, 0), 0, "{style:?}");
        for w in [1u16, 10, 200, u16::MAX] {
            let n = bands_wanted(style, w);
            assert!((1..=MAX_BANDS).contains(&n), "{style:?} w={w} n={n}");
        }
    }
    assert_eq!(bands_wanted(VisualStyle::Bars, 1), 1);
    assert_eq!(bands_wanted(VisualStyle::Bars, 10), 5);
    assert_eq!(bands_wanted(VisualStyle::Dots, 4), 4);
    assert_eq!(bands_wanted(VisualStyle::Area, 10), 10);
    assert_eq!(bands_wanted(VisualStyle::Area, 200), MAX_BANDS);
    assert!(bands_wanted(VisualStyle::Wave, 200) <= 16);
    assert!(bands_wanted(VisualStyle::Vu, 200) <= 16);
}

#[test]
fn resample_pools_and_interpolates() {
    assert_eq!(resample(&[0.0, 1.0, 0.0, 0.5], 2), vec![1.0, 0.5]);
    assert_eq!(resample(&[0.0, 1.0], 5), vec![0.0, 0.25, 0.5, 0.75, 1.0]);
    assert_eq!(resample(&[0.4], 3), vec![0.4, 0.4, 0.4]);
    assert_eq!(resample(&[], 3), vec![0.0; 3]);
    assert!(resample(&[1.0], 0).is_empty());
}

#[test]
fn from_analysis_copies_outputs() {
    use crate::operations::spectrum::SpectrumConfig;
    let analyzer = SpectrumAnalyzer::new(SpectrumConfig {
        bands: 12,
        ..Default::default()
    });
    let levels = Levels {
        left_peak: 0.4,
        right_peak: 0.6,
        ..Default::default()
    };
    let d = VisualizerData::from_analysis(&analyzer, vec![0.1, -0.1], levels);
    assert_eq!(d.bands.len(), 12);
    assert_eq!(d.peaks.len(), analyzer.peaks().len());
    assert_eq!(d.waveform, vec![0.1, -0.1]);
    assert_eq!((d.level_l, d.level_r), (0.4, 0.6));
}

/// Manual preview: `cargo test preview_all_styles -- --ignored --nocapture`.
#[test]
#[ignore = "prints a visual preview; run manually with --nocapture"]
fn preview_all_styles() {
    let n = 60usize;
    let bands: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            let base = 0.85 * (1.0 - t).powf(1.4) + 0.05;
            let bumps = [(0.08, 0.15), (0.3, 0.12), (0.55, 0.1)]
                .iter()
                .map(|(c, a)| a * (-((t - c) * 25.0).powi(2)).exp())
                .sum::<f32>();
            (base + bumps).min(1.0)
        })
        .collect();
    let data = VisualizerData {
        peaks: bands.iter().map(|b| (b + 0.12).min(1.0)).collect(),
        bands,
        waveform: sine(600, 4.0)
            .iter()
            .zip(sine(600, 11.0))
            .map(|(a, b)| 0.6 * a + 0.25 * b)
            .collect(),
        level_l: 0.62,
        level_r: 0.48,
        hold_l: 0.8,
        hold_r: 0.7,
    };
    for style in VisualStyle::ALL {
        println!(
            "--- {} ---\n{}",
            style.name(),
            grid(&draw(style, &data, 60, 10))
        );
    }
}

fn fg_of(buf: &Buffer, x: u16, y: u16) -> ratatui::style::Color {
    buf[(x, y)].fg
}

fn draw_with(theme: &Theme, style: VisualStyle, data: &VisualizerData, w: u16, h: u16) -> Buffer {
    let area = Rect::new(0, 0, w, h);
    let mut buf = Buffer::empty(area);
    render(style, data, theme, area, &mut buf);
    buf
}

#[test]
fn vu_zone_thresholds() {
    use crate::tui::theme::VuZone;
    assert_eq!(vu::vu_zone(0.0), VuZone::Ok);
    assert_eq!(vu::vu_zone(0.6), VuZone::Ok);
    assert_eq!(vu::vu_zone(0.61), VuZone::Warn);
    assert_eq!(vu::vu_zone(0.85), VuZone::Warn);
    assert_eq!(vu::vu_zone(0.86), VuZone::Clip);
    assert_eq!(vu::vu_zone(1.0), VuZone::Clip);
}

#[test]
fn bars_follow_the_gradient_from_bottom_to_top() {
    let theme = Theme::preset("nord").unwrap();
    let buf = draw_with(&theme, VisualStyle::Bars, &data_bands(vec![1.0]), 1, 6);
    let colors: Vec<_> = (0..6).map(|y| fg_of(&buf, 0, 5 - y)).collect();
    assert_eq!(colors[0], theme.level_color(0.5 / 6.0));
    assert_eq!(colors[5], theme.level_color(5.5 / 6.0));
    assert_ne!(colors[0], colors[5]);
    // Rgb stops interpolate: no two neighbours jump over the middle stop.
    assert!(colors.windows(2).all(|w| w[0] != w[1]));
}

#[test]
fn non_rgb_gradient_steps_in_thirds() {
    let theme = Theme::preset("high-contrast").unwrap();
    let buf = draw_with(&theme, VisualStyle::Bars, &data_bands(vec![1.0]), 1, 6);
    let colors: Vec<_> = (0..6).map(|y| fg_of(&buf, 0, 5 - y)).collect();
    assert_eq!(colors[0], theme.viz_low);
    assert_eq!(colors[1], theme.viz_low);
    assert_eq!(colors[2], theme.viz_mid);
    assert_eq!(colors[3], theme.viz_mid);
    assert_eq!(colors[4], theme.viz_high);
    assert_eq!(colors[5], theme.viz_high);
}

#[test]
fn two_presets_color_the_same_data_differently() {
    let data = VisualizerData {
        bands: ramp_data(16),
        peaks: vec![1.0; 16],
        waveform: (0..64).map(|i| (i as f32 / 5.0).sin()).collect(),
        level_l: 0.9,
        level_r: 0.5,
        hold_l: 1.0,
        hold_r: 0.7,
    };
    let (a, b) = (
        Theme::preset("nord").unwrap(),
        Theme::preset("dracula").unwrap(),
    );
    for style in VisualStyle::ALL {
        let x = draw_with(&a, style, &data, 24, 6);
        let y = draw_with(&b, style, &data, 24, 6);
        assert_eq!(grid(&x), grid(&y), "{style:?}: same glyphs");
        assert_ne!(x, y, "{style:?}: different colors");
    }
}

#[test]
fn default_theme_colors_every_part_with_the_single_accent() {
    let theme = Theme::default();
    let data = VisualizerData {
        bands: ramp_data(8),
        peaks: vec![1.0; 8],
        waveform: (0..32).map(|i| (i as f32 / 3.0).sin()).collect(),
        level_l: 1.0,
        level_r: 1.0,
        hold_l: 1.0,
        hold_r: 1.0,
    };
    for style in VisualStyle::ALL {
        let buf = draw_with(&theme, style, &data, 16, 6);
        for y in 0..6 {
            for x in 0..16 {
                let c = &buf[(x, y)];
                assert!(
                    c.symbol() == " "
                        || matches!(
                            c.fg,
                            ratatui::style::Color::Blue | ratatui::style::Color::Reset
                        ),
                    "{style:?} ({x},{y}) {:?}",
                    c.fg
                );
            }
        }
    }
}

#[test]
fn vu_segments_use_zone_colors_and_the_hold_color() {
    let theme = Theme::preset("dracula").unwrap();
    let data = VisualizerData {
        level_l: 1.0,
        hold_l: 1.0,
        level_r: 0.5,
        hold_r: 0.9,
        ..Default::default()
    };
    // Width 22 with labels: 20 segments starting at x = 2.
    let buf = draw_with(&theme, VisualStyle::Vu, &data, 22, 2);
    assert_eq!(fg_of(&buf, 2, 0), theme.viz_vu_ok);
    assert_eq!(fg_of(&buf, 2 + 14, 0), theme.viz_vu_warn);
    assert_eq!(fg_of(&buf, 21, 0), theme.viz_vu_clip);
    // R meter: 10 filled, hold tick at segment 18.
    assert_eq!(fg_of(&buf, 2, 1), theme.viz_vu_ok);
    assert_eq!(fg_of(&buf, 2 + 17, 1), theme.viz_vu_hold);
    assert!(buf[(2 + 17, 1)].modifier.contains(Modifier::BOLD));
}

#[test]
fn dots_and_wave_use_their_own_roles() {
    let theme = Theme::preset("gruvbox").unwrap();
    let dots = draw_with(&theme, VisualStyle::Dots, &data_bands(vec![0.5]), 1, 10);
    // Body dot is dim, head dot uses the gradient, peak marker uses viz_peak.
    assert_eq!(fg_of(&dots, 0, 9), theme.viz_dim);
    let peak_row = (0..10).find(|y| sym(&dots, 0, *y) == "▪").unwrap();
    assert_eq!(fg_of(&dots, 0, peak_row), theme.viz_peak);
    let data = VisualizerData {
        waveform: vec![0.0; 16],
        ..Default::default()
    };
    let wave = draw_with(&theme, VisualStyle::Wave, &data, 8, 3);
    let lit: Vec<_> = (0..3)
        .flat_map(|y| (0..8).map(move |x| (x, y)))
        .filter(|&(x, y)| is_braille(&sym(&wave, x, y)))
        .collect();
    assert!(!lit.is_empty());
    assert!(lit
        .iter()
        .all(|&(x, y)| fg_of(&wave, x, y) == theme.viz_wave));
}
