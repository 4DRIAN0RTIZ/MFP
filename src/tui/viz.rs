//! Per-frame driver of the audio visualizer.
//!
//! Runs on the UI thread, once per loop iteration and never inside `draw`:
//! it reads the audio tap, advances the spectrum analyzer and leaves a plain
//! [`VisualizerData`] in the app state for the renderer.
//!
//! Every failure mode degrades to a silent visualizer: a poisoned tap lock
//! yields empty snapshots and a zero sample rate, which the engine treats as
//! "no audio" and feeds silence, so the bars fall to zero on their own. Paused
//! playback, stalled or ended streams, a cleared tap and track switches all
//! take the same path. The audio itself is never touched.

use std::time::{Duration, Instant};

use crate::audio_tap::{Levels, TapHandle, SUGGESTED_DELAY_MS};
use crate::operations::spectrum::{SpectrumAnalyzer, SpectrumConfig, DEFAULT_FFT_SIZE};

use super::widgets::visualizer::{bands_wanted, VisualStyle, VisualizerData};

/// Samples per analysis window.
pub const FFT_SIZE: usize = DEFAULT_FFT_SIZE;
/// Newest mono samples handed to the waveform (Wave) style.
pub const WAVE_SAMPLES: usize = 1024;
/// Event loop tick while the visualizer animates.
pub const ANIM_TICK: Duration = Duration::from_millis(50);
/// Event loop tick otherwise (low idle CPU).
pub const IDLE_TICK: Duration = Duration::from_millis(100);
/// Shortest frame time fed to the smoothing, in seconds.
const MIN_DT: f32 = 0.005;
/// Longest frame time fed to the smoothing, in seconds.
const MAX_DT: f32 = 0.25;
/// Frame time assumed on the first frame after being idle.
const FIRST_DT: f32 = 0.05;
/// Level (dBFS) shown as an empty meter.
pub const METER_FLOOR_DB: f32 = -60.0;
/// How fast the VU bar falls, in meter units per second.
const METER_FALL_PER_SEC: f32 = 2.5;
/// How fast the VU peak-hold tick falls, in meter units per second.
const HOLD_FALL_PER_SEC: f32 = 0.5;

/// Event loop tick: fast while the visualizer is on screen and audio plays,
/// slow otherwise.
pub fn poll_interval(viz_visible: bool, playing: bool) -> Duration {
    if viz_visible && playing {
        ANIM_TICK
    } else {
        IDLE_TICK
    }
}

/// Maps a linear peak amplitude to a `0..=1` meter position on a dB scale
/// from [`METER_FLOOR_DB`] to 0 dBFS. Silence and non-finite values give 0.
pub fn level_to_meter(peak: f32) -> f32 {
    if !peak.is_finite() || peak <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * peak.log10();
    ((db - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0)
}

/// Next value of a level that jumps up instantly and falls at `fall_per_sec`.
pub fn fall_towards(current: f32, target: f32, dt: f32, fall_per_sec: f32) -> f32 {
    let target = if target.is_finite() { target } else { 0.0 };
    let current = if current.is_finite() { current } else { 0.0 };
    if target >= current {
        target
    } else {
        (current - fall_per_sec * dt).max(target)
    }
}

/// Converts an elapsed time to a smoothing step in seconds, clamped to
/// `MIN_DT..=MAX_DT` so a stall never makes the bars jump.
pub fn frame_dt(elapsed: Duration) -> f32 {
    elapsed.as_secs_f32().clamp(MIN_DT, MAX_DT)
}

/// One read of the audio tap.
#[derive(Debug, Clone, Default)]
pub struct TapFrame {
    /// Newest [`FFT_SIZE`] mono samples, shifted back by the output delay.
    pub samples: Vec<f32>,
    /// Peak levels since the previous read.
    pub levels: Levels,
    /// Sample rate of the tapped source; 0 when nothing is being tapped.
    pub sample_rate: u32,
    /// Frames tapped since the tap was last cleared.
    pub frames_written: u64,
}

/// Reads the tap once. Calls [`TapHandle::levels`] exactly once (it resets the
/// accumulator), so call this at most once per frame.
pub fn read_tap(tap: &TapHandle) -> TapFrame {
    let sample_rate = tap.sample_rate();
    let delay = TapHandle::delay_samples(sample_rate, SUGGESTED_DELAY_MS);
    TapFrame {
        samples: tap.snapshot_delayed(FFT_SIZE, delay),
        levels: tap.levels(),
        sample_rate,
        frames_written: tap.frames_written(),
    }
}

/// Analyzer plus the small pieces of state the styles need between frames.
pub struct VizEngine {
    analyzer: SpectrumAnalyzer,
    last_frames: u64,
    last_tick: Option<Instant>,
    meter_l: f32,
    meter_r: f32,
    hold_l: f32,
    hold_r: f32,
}

impl Default for VizEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl VizEngine {
    /// Creates an engine with no bands yet (the first frame sizes them).
    pub fn new() -> Self {
        let analyzer = SpectrumAnalyzer::new(SpectrumConfig {
            fft_size: FFT_SIZE,
            bands: 0,
            ..SpectrumConfig::default()
        });
        Self {
            analyzer,
            last_frames: 0,
            last_tick: None,
            meter_l: 0.0,
            meter_r: 0.0,
            hold_l: 0.0,
            hold_r: 0.0,
        }
    }

    /// Forgets everything (visualizer hidden or off). The next frame starts
    /// from silence with a default frame time.
    pub fn idle(&mut self) {
        self.analyzer.reset();
        self.meter_l = 0.0;
        self.meter_r = 0.0;
        self.hold_l = 0.0;
        self.hold_r = 0.0;
        self.last_tick = None;
    }

    /// Reads the tap and produces the frame to draw at `now`.
    pub fn tick(
        &mut self,
        tap: &TapHandle,
        style: VisualStyle,
        width: u16,
        paused: bool,
        now: Instant,
    ) -> VisualizerData {
        let frame = read_tap(tap);
        self.step(style, width, paused, &frame, now)
    }

    /// Advances the analysis by one frame.
    ///
    /// Silence is fed (so bars fall through the analyzer's release) when the
    /// player is paused, the tap has no sample rate (cleared or never used) or
    /// `frames_written` did not move since the previous frame (stalled
    /// stream); otherwise the delayed window is analyzed.
    pub fn step(
        &mut self,
        style: VisualStyle,
        width: u16,
        paused: bool,
        frame: &TapFrame,
        now: Instant,
    ) -> VisualizerData {
        let dt = match self.last_tick {
            Some(prev) => frame_dt(now.saturating_duration_since(prev)),
            None => FIRST_DT,
        };
        self.last_tick = Some(now);

        let wanted = bands_wanted(style, width);
        if self.analyzer.band_count() != wanted {
            self.analyzer.set_band_count(wanted);
        }
        if frame.sample_rate > 0 {
            self.analyzer.set_sample_rate(frame.sample_rate);
        }
        let flowing = !paused && frame.sample_rate > 0 && frame.frames_written != self.last_frames;
        self.last_frames = frame.frames_written;

        let samples: &[f32] = if flowing { &frame.samples } else { &[] };
        self.analyzer.analyze(samples, dt);

        let levels = if flowing {
            frame.levels
        } else {
            Levels::default()
        };
        let target_l = level_to_meter(levels.left_peak);
        let target_r = level_to_meter(levels.right_peak);
        self.meter_l = fall_towards(self.meter_l, target_l, dt, METER_FALL_PER_SEC);
        self.meter_r = fall_towards(self.meter_r, target_r, dt, METER_FALL_PER_SEC);
        self.hold_l = fall_towards(self.hold_l, self.meter_l, dt, HOLD_FALL_PER_SEC);
        self.hold_r = fall_towards(self.hold_r, self.meter_r, dt, HOLD_FALL_PER_SEC);

        let waveform = if flowing && style == VisualStyle::Wave {
            let from = samples.len().saturating_sub(WAVE_SAMPLES);
            samples[from..].to_vec()
        } else {
            Vec::new()
        };
        let mut data = VisualizerData::from_analysis(
            &self.analyzer,
            waveform,
            Levels {
                left_peak: self.meter_l,
                right_peak: self.meter_r,
                ..Levels::default()
            },
        );
        data.hold_l = self.hold_l;
        data.hold_r = self.hold_r;
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 44_100;

    fn sine(freq: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.8 * (2.0 * std::f32::consts::PI * freq * i as f32 / SR as f32).sin())
            .collect()
    }

    fn frame(frames_written: u64) -> TapFrame {
        TapFrame {
            samples: sine(1000.0, FFT_SIZE),
            levels: Levels {
                left_peak: 0.5,
                right_peak: 0.25,
                ..Levels::default()
            },
            sample_rate: SR,
            frames_written,
        }
    }

    fn max_band(d: &VisualizerData) -> f32 {
        d.bands.iter().copied().fold(0.0, f32::max)
    }

    #[test]
    fn poll_interval_is_fast_only_when_visible_and_playing() {
        assert_eq!(poll_interval(true, true), ANIM_TICK);
        assert_eq!(poll_interval(true, false), IDLE_TICK);
        assert_eq!(poll_interval(false, true), IDLE_TICK);
        assert_eq!(poll_interval(false, false), IDLE_TICK);
        assert_eq!(ANIM_TICK, Duration::from_millis(50));
        assert_eq!(IDLE_TICK, Duration::from_millis(100));
    }

    #[test]
    fn meter_curve_maps_db_range_to_unit() {
        assert_eq!(level_to_meter(0.0), 0.0);
        assert_eq!(level_to_meter(-1.0), 0.0);
        assert_eq!(level_to_meter(f32::NAN), 0.0);
        assert_eq!(level_to_meter(f32::INFINITY), 0.0);
        assert!((level_to_meter(1.0) - 1.0).abs() < 1e-6);
        assert_eq!(level_to_meter(4.0), 1.0, "clamped above 0 dBFS");
        // -6 dBFS is 90% of a -60..0 scale.
        assert!((level_to_meter(0.501_187) - 0.9).abs() < 1e-3);
        // -60 dBFS and below is empty.
        assert!(level_to_meter(0.001) < 1e-4);
        assert_eq!(level_to_meter(1e-6), 0.0);
        // Monotonic.
        let mut prev = -1.0;
        for i in 1..=100 {
            let m = level_to_meter(i as f32 / 100.0);
            assert!(m >= prev);
            prev = m;
        }
    }

    #[test]
    fn fall_towards_jumps_up_and_decays_slowly() {
        assert_eq!(fall_towards(0.2, 0.7, 0.05, 0.5), 0.7);
        let down = fall_towards(0.8, 0.0, 0.1, 0.5);
        assert!((down - 0.75).abs() < 1e-6);
        // Never drops below the target.
        assert_eq!(fall_towards(0.3, 0.29, 1.0, 0.5), 0.29);
        // Full decay takes 1 / fall seconds.
        let mut v = 1.0;
        for _ in 0..41 {
            v = fall_towards(v, 0.0, 0.05, 0.5);
        }
        assert_eq!(v, 0.0);
        assert_eq!(fall_towards(f32::NAN, f32::NAN, 0.1, 0.5), 0.0);
    }

    #[test]
    fn frame_dt_is_clamped() {
        assert_eq!(frame_dt(Duration::ZERO), MIN_DT);
        assert_eq!(frame_dt(Duration::from_secs(5)), MAX_DT);
        assert!((frame_dt(Duration::from_millis(50)) - 0.05).abs() < 1e-6);
    }

    #[test]
    fn playing_audio_lights_the_bands_and_meters() {
        let mut eng = VizEngine::new();
        let t0 = Instant::now();
        let mut data = eng.step(VisualStyle::Bars, 40, false, &frame(1), t0);
        for i in 1..6u64 {
            data = eng.step(
                VisualStyle::Bars,
                40,
                false,
                &frame(1 + i * 512),
                t0 + Duration::from_millis(50 * i),
            );
        }
        assert_eq!(data.bands.len(), 20);
        assert!(max_band(&data) > 0.3, "{:?}", data.bands);
        assert!(data.level_l > data.level_r && data.level_r > 0.0);
        assert!(data.hold_l >= data.level_l);
        assert!(data.waveform.is_empty(), "only Wave asks for samples");
    }

    #[test]
    fn wave_style_gets_the_newest_samples() {
        let mut eng = VizEngine::new();
        let data = eng.step(VisualStyle::Wave, 40, false, &frame(1), Instant::now());
        assert_eq!(data.waveform.len(), WAVE_SAMPLES);
        assert_eq!(data.bands.len(), 8);
        let f = frame(1);
        assert_eq!(data.waveform, f.samples[FFT_SIZE - WAVE_SAMPLES..]);
    }

    /// Runs `n` frames of `dt_ms` and returns the last one.
    fn run(
        eng: &mut VizEngine,
        t: &mut Instant,
        n: usize,
        dt_ms: u64,
        paused: bool,
        mut fr: impl FnMut(usize) -> TapFrame,
    ) -> VisualizerData {
        let mut last = VisualizerData::default();
        for i in 0..n {
            *t += Duration::from_millis(dt_ms);
            last = eng.step(VisualStyle::Bars, 40, paused, &fr(i), *t);
        }
        last
    }

    #[test]
    fn pause_decays_bars_to_zero() {
        let mut eng = VizEngine::new();
        let mut t = Instant::now();
        let live = run(&mut eng, &mut t, 10, 50, false, |i| {
            frame(1 + i as u64 * 512)
        });
        assert!(max_band(&live) > 0.3);
        // Same tap contents, frames still moving, but the player is paused.
        let after = run(&mut eng, &mut t, 60, 50, true, |i| {
            frame(10_000 + i as u64 * 512)
        });
        // The analyzer's release is exponential: below 0.001 no bar shows.
        assert!(max_band(&after) < 0.001, "{:?}", after.bands);
        assert_eq!(after.level_l, 0.0);
        assert_eq!(after.level_r, 0.0);
        assert!(after.peaks.iter().all(|p| *p < 0.001), "{:?}", after.peaks);
        // Bars fall smoothly, not in a single step.
        let mut eng = VizEngine::new();
        let mut t = Instant::now();
        run(&mut eng, &mut t, 10, 50, false, |i| {
            frame(1 + i as u64 * 512)
        });
        let one = run(&mut eng, &mut t, 1, 50, true, |_| frame(9_999));
        assert!(max_band(&one) > 0.0, "release is gradual");
    }

    #[test]
    fn stalled_frames_decay_like_pause() {
        let mut eng = VizEngine::new();
        let mut t = Instant::now();
        run(&mut eng, &mut t, 10, 50, false, |i| {
            frame(1 + i as u64 * 512)
        });
        // frames_written stops moving (stream ended or stuck buffering).
        let stalled = run(&mut eng, &mut t, 60, 50, false, |_| frame(4_609));
        assert!(max_band(&stalled) < 0.001);
        assert_eq!(stalled.level_l, 0.0);
    }

    #[test]
    fn zero_sample_rate_is_silent_and_safe() {
        let mut eng = VizEngine::new();
        let cleared = TapFrame::default();
        let mut t = Instant::now();
        for style in VisualStyle::ALL {
            for width in [0u16, 1, 3, 80, 500] {
                t += Duration::from_millis(50);
                let d = eng.step(style, width, false, &cleared, t);
                assert_eq!(max_band(&d), 0.0);
                assert_eq!(d.level_l, 0.0);
                assert!(d.waveform.is_empty());
            }
        }
        // Frames flowing but no rate is still silence, never a division by zero.
        let odd = TapFrame {
            frames_written: 77,
            samples: sine(440.0, FFT_SIZE),
            ..TapFrame::default()
        };
        let d = eng.step(VisualStyle::Bars, 40, false, &odd, t);
        assert_eq!(max_band(&d), 0.0);
    }

    #[test]
    fn band_count_follows_style_and_width() {
        let mut eng = VizEngine::new();
        let now = Instant::now();
        let f = frame(1);
        let d = eng.step(VisualStyle::Bars, 60, false, &f, now);
        assert_eq!(d.bands.len(), bands_wanted(VisualStyle::Bars, 60));
        let d = eng.step(VisualStyle::Bars, 30, false, &f, now);
        assert_eq!(d.bands.len(), bands_wanted(VisualStyle::Bars, 30));
        let d = eng.step(VisualStyle::Area, 30, false, &f, now);
        assert_eq!(d.bands.len(), 30);
        let d = eng.step(VisualStyle::Vu, 30, false, &f, now);
        assert_eq!(d.bands.len(), bands_wanted(VisualStyle::Vu, 30));
        let d = eng.step(VisualStyle::Bars, 0, false, &f, now);
        assert!(d.bands.is_empty());
    }

    #[test]
    fn idle_resets_state() {
        let mut eng = VizEngine::new();
        let mut t = Instant::now();
        run(&mut eng, &mut t, 10, 50, false, |i| {
            frame(1 + i as u64 * 512)
        });
        eng.idle();
        let d = eng.step(VisualStyle::Bars, 40, true, &TapFrame::default(), t);
        assert_eq!(max_band(&d), 0.0);
        assert_eq!(d.hold_l, 0.0);
    }

    #[test]
    fn vu_hold_lags_behind_a_falling_meter() {
        let mut eng = VizEngine::new();
        let mut t = Instant::now();
        run(&mut eng, &mut t, 3, 50, false, |i| {
            frame(1 + i as u64 * 512)
        });
        let after = run(&mut eng, &mut t, 4, 50, true, |_| frame(9_999));
        assert!(after.hold_l > after.level_l, "{after:?}");
    }

    #[test]
    fn tick_reads_a_real_tap_and_survives_an_empty_one() {
        let tap = TapHandle::new();
        let mut eng = VizEngine::new();
        let d = eng.tick(&tap, VisualStyle::Bars, 40, false, Instant::now());
        assert_eq!(max_band(&d), 0.0);
        assert_eq!(d.bands.len(), 20);
    }
}
