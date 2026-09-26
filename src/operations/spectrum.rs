//! Pure spectrum analysis for the audio visualizer (no I/O, never prints).
//!
//! Pipeline, one call to [`SpectrumAnalyzer::analyze`]:
//!
//! 1. Take the newest `fft_size` mono samples (zero-padded at the front if
//!    fewer are available) and multiply them by a Hann window.
//! 2. Run an in-place iterative radix-2 complex FFT ([`Fft`]).
//! 3. Turn the first `fft_size / 2 + 1` bins into linear amplitudes
//!    (a full-scale sine gives 1.0, see [`SpectrumAnalyzer::analyze`]).
//! 4. Group bins into logarithmically spaced bands between
//!    [`MIN_FREQ_HZ`] and `min(`[`MAX_FREQ_HZ`]`, Nyquist)`.
//! 5. Convert each band to decibels and map `DB_FLOOR..DB_CEIL` onto
//!    `0.0..=1.0`.
//! 6. Smooth with a fast attack and a slow release, and track peak-hold
//!    values.
//!
//! Everything here is plain arithmetic on slices so it can be unit tested
//! with synthesized signals; the caller owns threading and timing.
//!
//! The visualizer is not wired into the UI yet, hence the `dead_code` allow.

// Used by phase C (TUI integration) of feature #9.
#![allow(dead_code)]

/// Default FFT size.
///
/// A power of two is required by the radix-2 algorithm. 2048 samples is
/// ~46 ms at 44.1 kHz: ~21.5 Hz per bin, enough to separate bass notes
/// while keeping one pass cheap and the time resolution below a UI frame
/// budget worth of smearing.
pub const DEFAULT_FFT_SIZE: usize = 2048;
/// Lowest band edge in Hz.
pub const MIN_FREQ_HZ: f32 = 40.0;
/// Highest band edge in Hz (clamped to Nyquist for low sample rates).
pub const MAX_FREQ_HZ: f32 = 16_000.0;
/// Level (dB relative to a full-scale sine) mapped to 0.0.
pub const DB_FLOOR: f32 = -80.0;
/// Level (dB relative to a full-scale sine) mapped to 1.0. Music rarely
/// puts a single band above -20 dB, so a ceiling of -10 keeps bars moving.
pub const DB_CEIL: f32 = -10.0;
/// Spectral tilt in dB per octave above [`TILT_REF_HZ`]. Music energy falls
/// roughly 3 dB/octave, so without compensation the treble would stay flat.
pub const TILT_DB_PER_OCTAVE: f32 = 3.0;
/// Frequency where the tilt is 0 dB.
pub const TILT_REF_HZ: f32 = 1000.0;
/// Attack time constant in seconds (rise towards a louder target).
pub const ATTACK_SECS: f32 = 0.03;
/// Release time constant in seconds (fall towards a quieter target).
pub const RELEASE_SECS: f32 = 0.30;
/// Time a peak marker stays put before it starts to fall.
pub const PEAK_HOLD_SECS: f32 = 0.40;
/// Fall speed of a peak marker, in normalized units per second.
pub const PEAK_FALL_PER_SEC: f32 = 0.80;
/// Smallest sample rate the analyzer accepts (lower values are clamped so
/// the band range stays meaningful).
const MIN_SAMPLE_RATE: u32 = 8_000;
/// Amplitude floor before taking the logarithm.
const MIN_AMPLITUDE: f32 = 1e-9;

/// Periodic Hann window of length `n`: `w[i] = 0.5 - 0.5 cos(2 pi i / n)`.
///
/// Tapering the block to zero at both ends removes the discontinuity
/// created by cutting the signal, which would otherwise smear energy
/// across all bins (spectral leakage). The periodic form (period `n`, not
/// `n - 1`) is the right one for spectral analysis; its mean is 0.5.
pub fn hann_window(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let phase = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            (0.5 - 0.5 * phase.cos()) as f32
        })
        .collect()
}

/// Precomputed plan for an in-place iterative radix-2 (decimation in time)
/// complex FFT of a fixed power-of-two size.
///
/// Computes `X[k] = sum_n x[n] e^{-2 pi i k n / N}` in `O(N log N)`:
/// samples are first permuted into bit-reversed order, then `log2 N`
/// stages of butterflies combine transforms of size 1, 2, 4, ... N.
pub struct Fft {
    n: usize,
    /// `cos(2 pi k / N)` for `k < N/2`.
    cos: Vec<f32>,
    /// `-sin(2 pi k / N)` for `k < N/2` (so `cos + i*sin` is `e^{-2 pi i k/N}`).
    sin: Vec<f32>,
    rev: Vec<u32>,
}

impl Fft {
    /// Builds a plan for `n` points. Returns `None` unless `n` is a power
    /// of two and at least 2.
    pub fn new(n: usize) -> Option<Self> {
        if n < 2 || !n.is_power_of_two() {
            return None;
        }
        let bits = n.trailing_zeros();
        let rev = (0..n as u32)
            .map(|i| i.reverse_bits() >> (32 - bits))
            .collect();
        let (cos, sin) = (0..n / 2)
            .map(|k| {
                let a = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
                (a.cos() as f32, -a.sin() as f32)
            })
            .unzip();
        Some(Self { n, cos, sin, rev })
    }

    /// Transform size.
    pub fn len(&self) -> usize {
        self.n
    }

    /// Forward transform in place. Both slices must have length
    /// [`Fft::len`]; otherwise nothing is done.
    pub fn transform(&self, re: &mut [f32], im: &mut [f32]) {
        let n = self.n;
        if re.len() != n || im.len() != n {
            return;
        }
        for i in 0..n {
            let j = self.rev[i] as usize;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..half {
                    let (wr, wi) = (self.cos[k * step], self.sin[k * step]);
                    let a = start + k;
                    let b = a + half;
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len *= 2;
        }
    }
}

/// Writes `sqrt(re^2 + im^2)` for the first `out.len()` bins into `out`.
pub fn magnitude_spectrum(re: &[f32], im: &[f32], out: &mut [f32]) {
    for (o, (r, i)) in out.iter_mut().zip(re.iter().zip(im.iter())) {
        *o = r.hypot(*i);
    }
}

/// Returns `(rms, peak)` of `samples`; `(0.0, 0.0)` for an empty slice.
pub fn rms_and_peak(samples: &[f32]) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let mut sum = 0.0f64;
    let mut peak = 0.0f32;
    for &s in samples {
        let s = if s.is_finite() { s } else { 0.0 };
        sum += (s as f64) * (s as f64);
        peak = peak.max(s.abs());
    }
    (((sum / samples.len() as f64).sqrt()) as f32, peak)
}

/// Band edges in Hz: `bands + 1` logarithmically spaced values from
/// [`MIN_FREQ_HZ`] to `min(`[`MAX_FREQ_HZ`]`, Nyquist)`, i.e.
/// `e[i] = lo * (hi / lo)^(i / bands)`. Empty when `bands == 0`.
///
/// A log scale matches pitch perception: each octave gets a similar
/// number of bands instead of the treble taking almost all of them.
pub fn band_edges(bands: usize, sample_rate: u32) -> Vec<f32> {
    if bands == 0 {
        return Vec::new();
    }
    let sr = sample_rate.max(MIN_SAMPLE_RATE) as f32;
    let lo = MIN_FREQ_HZ;
    let hi = MAX_FREQ_HZ.min(sr / 2.0);
    let ratio = hi / lo;
    (0..=bands)
        .map(|i| lo * ratio.powf(i as f32 / bands as f32))
        .collect()
}

/// One smoothing step: exponential approach to `target` with time
/// constant `attack` when rising and `release` when falling.
///
/// `value' = target + (value - target) * exp(-dt / tau)`; a non-positive
/// `dt` leaves the value unchanged and a non-positive tau snaps to the
/// target.
pub fn smooth(value: f32, target: f32, dt: f32, attack: f32, release: f32) -> f32 {
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    let tau = if target > value { attack } else { release };
    if tau <= 0.0 {
        return target;
    }
    target + (value - target) * (-dt / tau).exp()
}

/// Analyzer configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectrumConfig {
    /// FFT size; rounded up to a power of two (minimum 4).
    pub fft_size: usize,
    /// Sample rate of the mono input in Hz.
    pub sample_rate: u32,
    /// Number of output bands (0 is allowed and yields no bands).
    pub bands: usize,
    /// Attack time constant in seconds.
    pub attack_secs: f32,
    /// Release time constant in seconds.
    pub release_secs: f32,
    /// Peak hold time in seconds.
    pub peak_hold_secs: f32,
    /// Peak fall speed in normalized units per second.
    pub peak_fall_per_sec: f32,
}

impl Default for SpectrumConfig {
    fn default() -> Self {
        Self {
            fft_size: DEFAULT_FFT_SIZE,
            sample_rate: 44_100,
            bands: 32,
            attack_secs: ATTACK_SECS,
            release_secs: RELEASE_SECS,
            peak_hold_secs: PEAK_HOLD_SECS,
            peak_fall_per_sec: PEAK_FALL_PER_SEC,
        }
    }
}

/// Stateful spectrum analyzer: FFT plan, window, scratch buffers and the
/// smoothed per-band state. Reuses its buffers, so `analyze` does not
/// allocate.
pub struct SpectrumAnalyzer {
    cfg: SpectrumConfig,
    fft: Fft,
    window: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    /// Linear amplitudes for bins `0..=fft_size / 2`.
    amps: Vec<f32>,
    edges: Vec<f32>,
    bands: Vec<f32>,
    peaks: Vec<f32>,
    hold: Vec<f32>,
    rms: f32,
    peak_level: f32,
}

impl SpectrumAnalyzer {
    /// Creates an analyzer; invalid config values are sanitized (FFT size
    /// rounded to a power of two, sample rate clamped).
    pub fn new(mut cfg: SpectrumConfig) -> Self {
        cfg.fft_size = cfg.fft_size.max(4).next_power_of_two();
        cfg.sample_rate = cfg.sample_rate.max(MIN_SAMPLE_RATE);
        let n = cfg.fft_size;
        // `n` is a power of two >= 4, so the plan always exists; the
        // fallback keeps this constructor total without an unwrap.
        let fft = Fft::new(n).unwrap_or_else(|| Fft {
            n: 0,
            cos: Vec::new(),
            sin: Vec::new(),
            rev: Vec::new(),
        });
        let mut a = Self {
            fft,
            window: hann_window(n),
            re: vec![0.0; n],
            im: vec![0.0; n],
            amps: vec![0.0; n / 2 + 1],
            edges: Vec::new(),
            bands: Vec::new(),
            peaks: Vec::new(),
            hold: Vec::new(),
            rms: 0.0,
            peak_level: 0.0,
            cfg,
        };
        a.rebuild_bands();
        a
    }

    fn rebuild_bands(&mut self) {
        self.edges = band_edges(self.cfg.bands, self.cfg.sample_rate);
        self.bands = vec![0.0; self.cfg.bands];
        self.peaks = vec![0.0; self.cfg.bands];
        self.hold = vec![0.0; self.cfg.bands];
    }

    /// FFT size in use (after rounding).
    pub fn fft_size(&self) -> usize {
        self.cfg.fft_size
    }

    /// Current number of bands.
    pub fn band_count(&self) -> usize {
        self.cfg.bands
    }

    /// Changes the band count (e.g. when the UI is resized). Band state
    /// and peaks restart from zero.
    pub fn set_band_count(&mut self, bands: usize) {
        if bands != self.cfg.bands {
            self.cfg.bands = bands;
            self.rebuild_bands();
        }
    }

    /// Changes the input sample rate (new track). Band state restarts.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        let sr = sample_rate.max(MIN_SAMPLE_RATE);
        if sr != self.cfg.sample_rate {
            self.cfg.sample_rate = sr;
            self.rebuild_bands();
        }
    }

    /// Band edges in Hz (`band_count() + 1` values, or empty).
    pub fn edges(&self) -> &[f32] {
        &self.edges
    }

    /// Smoothed band values in `0.0..=1.0` from the last `analyze`.
    pub fn bands(&self) -> &[f32] {
        &self.bands
    }

    /// Peak-hold values in `0.0..=1.0`, never below the band value.
    pub fn peaks(&self) -> &[f32] {
        &self.peaks
    }

    /// RMS of the last analyzed window (linear, 0.0..=1.0 for full scale).
    pub fn rms(&self) -> f32 {
        self.rms
    }

    /// Absolute peak of the last analyzed window.
    pub fn peak_level(&self) -> f32 {
        self.peak_level
    }

    /// Zeroes bands, peaks and levels (pause, stop, new track).
    pub fn reset(&mut self) {
        self.bands.iter_mut().for_each(|v| *v = 0.0);
        self.peaks.iter_mut().for_each(|v| *v = 0.0);
        self.hold.iter_mut().for_each(|v| *v = 0.0);
        self.rms = 0.0;
        self.peak_level = 0.0;
    }

    /// Analyzes the newest `fft_size` samples of `samples` (mono, `f32`)
    /// and advances the smoothing by `dt` seconds. Returns the smoothed
    /// bands.
    ///
    /// Fewer samples than `fft_size` are zero-padded at the front (newest
    /// sample stays last); non-finite samples count as 0. Amplitudes are
    /// scaled by `4 / N`: a full-scale sine puts `A * N / 2` in its bin
    /// and the Hann window has a coherent gain of 0.5, so `mag * 4 / N`
    /// recovers the sine amplitude `A`. A band's level is
    /// `20 log10(amp) + tilt(f)` dB, mapped linearly from
    /// `DB_FLOOR..DB_CEIL` to `0..1`.
    ///
    /// Each band takes the largest bin whose frequency lies inside it;
    /// bands narrower than a bin (low frequencies with many bands)
    /// interpolate linearly between the two bins around their center.
    pub fn analyze(&mut self, samples: &[f32], dt: f32) -> &[f32] {
        let n = self.cfg.fft_size;
        let recent = if samples.len() > n {
            &samples[samples.len() - n..]
        } else {
            samples
        };
        let pad = n - recent.len();
        self.re[..pad].iter_mut().for_each(|v| *v = 0.0);
        for (dst, &s) in self.re[pad..].iter_mut().zip(recent) {
            *dst = if s.is_finite() { s } else { 0.0 };
        }
        let (rms, peak) = rms_and_peak(recent);
        self.rms = rms;
        self.peak_level = peak;

        for (v, w) in self.re.iter_mut().zip(&self.window) {
            *v *= *w;
        }
        self.im.iter_mut().for_each(|v| *v = 0.0);
        self.fft.transform(&mut self.re, &mut self.im);
        magnitude_spectrum(&self.re, &self.im, &mut self.amps);
        let scale = 4.0 / n as f32;
        self.amps.iter_mut().for_each(|a| *a *= scale);

        let bin_hz = self.cfg.sample_rate as f32 / n as f32;
        let last_bin = n / 2;
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        for b in 0..self.cfg.bands {
            let (f_lo, f_hi) = (self.edges[b], self.edges[b + 1]);
            let center = (f_lo * f_hi).sqrt();
            let first = (f_lo / bin_hz).ceil() as usize;
            let last = ((f_hi / bin_hz).ceil() as usize).saturating_sub(1);
            let amp = if first <= last {
                self.amps[first.min(last_bin)..=last.min(last_bin)]
                    .iter()
                    .fold(0.0f32, |m, &a| m.max(a))
            } else {
                let pos = center / bin_hz;
                let i = (pos.floor() as usize).min(last_bin);
                let j = (i + 1).min(last_bin);
                let frac = pos - pos.floor();
                self.amps[i] * (1.0 - frac) + self.amps[j] * frac
            };
            let db = 20.0 * amp.max(MIN_AMPLITUDE).log10()
                + TILT_DB_PER_OCTAVE * (center / TILT_REF_HZ).log2();
            let target = ((db - DB_FLOOR) / (DB_CEIL - DB_FLOOR)).clamp(0.0, 1.0);
            let v = smooth(
                self.bands[b],
                target,
                dt,
                self.cfg.attack_secs,
                self.cfg.release_secs,
            )
            .clamp(0.0, 1.0);
            self.bands[b] = v;

            if v >= self.peaks[b] {
                self.peaks[b] = v;
                self.hold[b] = self.cfg.peak_hold_secs;
            } else if self.hold[b] > 0.0 {
                self.hold[b] -= dt;
            } else {
                self.peaks[b] = (self.peaks[b] - self.cfg.peak_fall_per_sec * dt).max(v);
            }
        }
        &self.bands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const SR: u32 = 44_100;

    fn sine(freq: f32, n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (2.0 * PI * freq * i as f32 / SR as f32).sin())
            .collect()
    }

    fn analyzer(bands: usize) -> SpectrumAnalyzer {
        SpectrumAnalyzer::new(SpectrumConfig {
            bands,
            sample_rate: SR,
            ..SpectrumConfig::default()
        })
    }

    fn argmax(v: &[f32]) -> usize {
        v.iter()
            .enumerate()
            .fold(
                (0, f32::MIN),
                |(bi, bv), (i, &x)| {
                    if x > bv {
                        (i, x)
                    } else {
                        (bi, bv)
                    }
                },
            )
            .0
    }

    fn band_of(edges: &[f32], f: f32) -> usize {
        edges
            .windows(2)
            .position(|w| f >= w[0] && f < w[1])
            .unwrap()
    }

    #[test]
    fn hann_window_properties() {
        let n = 64;
        let w = hann_window(n);
        assert_eq!(w.len(), n);
        assert!(w[0].abs() < 1e-7);
        assert!((w[n / 2] - 1.0).abs() < 1e-6);
        for i in 1..n {
            assert!((w[i] - w[n - i]).abs() < 1e-6, "symmetry at {i}");
        }
        let mean: f32 = w.iter().sum::<f32>() / n as f32;
        assert!((mean - 0.5).abs() < 1e-6);
        assert!(w.iter().all(|&x| (0.0..=1.0).contains(&x)));
        assert!(hann_window(0).is_empty());
    }

    fn naive_dft(x: &[f32]) -> Vec<(f64, f64)> {
        let n = x.len();
        (0..n)
            .map(|k| {
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for (t, &v) in x.iter().enumerate() {
                    let a = -2.0 * std::f64::consts::PI * (k * t) as f64 / n as f64;
                    re += v as f64 * a.cos();
                    im += v as f64 * a.sin();
                }
                (re, im)
            })
            .collect()
    }

    #[test]
    fn fft_matches_naive_dft() {
        for n in [2usize, 4, 16, 64] {
            let input: Vec<f32> = (0..n)
                .map(|i| ((i * 7 + 3) % 11) as f32 / 5.0 - 1.0 + (i as f32 * 0.3).sin())
                .collect();
            let expected = naive_dft(&input);
            let mut re = input.clone();
            let mut im = vec![0.0; n];
            Fft::new(n).unwrap().transform(&mut re, &mut im);
            for k in 0..n {
                assert!((re[k] as f64 - expected[k].0).abs() < 1e-3, "n={n} re[{k}]");
                assert!((im[k] as f64 - expected[k].1).abs() < 1e-3, "n={n} im[{k}]");
            }
        }
    }

    #[test]
    fn fft_rejects_bad_sizes_and_lengths() {
        assert!(Fft::new(0).is_none());
        assert!(Fft::new(1).is_none());
        assert!(Fft::new(12).is_none());
        let fft = Fft::new(8).unwrap();
        assert_eq!(fft.len(), 8);
        let mut re = vec![1.0; 4];
        let mut im = vec![0.0; 4];
        fft.transform(&mut re, &mut im);
        assert_eq!(re, vec![1.0; 4]);
    }

    #[test]
    fn fft_of_impulse_is_flat() {
        let mut re = vec![0.0; 32];
        re[0] = 1.0;
        let mut im = vec![0.0; 32];
        Fft::new(32).unwrap().transform(&mut re, &mut im);
        assert!(re.iter().all(|&v| (v - 1.0).abs() < 1e-6));
        assert!(im.iter().all(|&v| v.abs() < 1e-6));
    }

    #[test]
    fn magnitude_is_hypot() {
        let mut out = [0.0; 2];
        magnitude_spectrum(&[3.0, 0.0, 9.0], &[4.0, -2.0, 9.0], &mut out);
        assert_eq!(out, [5.0, 2.0]);
    }

    #[test]
    fn band_edges_are_monotonic_and_cover_range() {
        for bands in [1usize, 2, 8, 32, 96] {
            let e = band_edges(bands, SR);
            assert_eq!(e.len(), bands + 1);
            assert!((e[0] - MIN_FREQ_HZ).abs() < 1e-3);
            assert!((e[bands] - MAX_FREQ_HZ).abs() < 1.0);
            assert!(e.windows(2).all(|w| w[1] > w[0]));
        }
        assert!(band_edges(0, SR).is_empty());
    }

    #[test]
    fn band_edges_clamp_to_nyquist() {
        let e = band_edges(8, 22_050);
        assert!((e[8] - 11_025.0).abs() < 1.0);
    }

    #[test]
    fn sines_land_in_expected_band() {
        let mut a = analyzer(16);
        let edges = a.edges().to_vec();
        for f in [440.0f32, 1000.0, 5000.0] {
            a.reset();
            let out = a.analyze(&sine(f, 2048, 0.5), 1.0).to_vec();
            let expected = band_of(&edges, f);
            assert_eq!(argmax(&out), expected, "{f} Hz");
            assert!(out[expected] > 0.5, "{f} Hz level {}", out[expected]);
        }
    }

    #[test]
    fn sines_land_near_expected_band_with_many_bands() {
        let mut a = analyzer(64);
        let edges = a.edges().to_vec();
        for f in [440.0f32, 1000.0, 5000.0] {
            a.reset();
            let out = a.analyze(&sine(f, 4096, 0.5), 1.0).to_vec();
            let expected = band_of(&edges, f) as i64;
            assert!((argmax(&out) as i64 - expected).abs() <= 1, "{f} Hz");
        }
    }

    #[test]
    fn silence_gives_zeros_without_nan() {
        let mut a = analyzer(24);
        let out = a.analyze(&vec![0.0; 2048], 0.1).to_vec();
        assert!(out.iter().all(|&v| v == 0.0));
        assert!(a.peaks().iter().all(|&v| v == 0.0));
        assert_eq!(a.rms(), 0.0);
        assert_eq!(a.peak_level(), 0.0);
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        let mut a = analyzer(8);
        assert_eq!(a.analyze(&[], 0.1).len(), 8);
        a.analyze(&[0.5], 0.1);
        a.analyze(&sine(1000.0, 100, 0.5), 0.1);
        let bad = [f32::NAN, f32::INFINITY, -f32::INFINITY, 0.3];
        let out = a.analyze(&bad, f32::NAN).to_vec();
        assert!(out.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
        let huge = vec![1e30f32; 4096];
        assert!(a.analyze(&huge, 0.1).iter().all(|v| v.is_finite()));
    }

    #[test]
    fn short_input_is_zero_padded_not_rejected() {
        let mut a = analyzer(16);
        let out = a.analyze(&sine(1000.0, 512, 0.9), 1.0).to_vec();
        assert!(out.iter().any(|&v| v > 0.3));
    }

    #[test]
    fn zero_and_one_band_counts() {
        let mut a = analyzer(0);
        assert!(a.analyze(&sine(440.0, 2048, 0.5), 0.1).is_empty());
        assert!(a.peaks().is_empty());
        a.set_band_count(1);
        let out = a.analyze(&sine(440.0, 2048, 0.5), 1.0).to_vec();
        assert_eq!(out.len(), 1);
        assert!(out[0] > 0.0);
    }

    #[test]
    fn band_count_changes_at_runtime() {
        let mut a = analyzer(8);
        a.analyze(&sine(440.0, 2048, 0.5), 1.0);
        for n in [96usize, 12, 0, 40] {
            a.set_band_count(n);
            assert_eq!(a.band_count(), n);
            assert_eq!(a.bands().len(), n);
            assert_eq!(a.peaks().len(), n);
            assert_eq!(a.edges().len(), if n == 0 { 0 } else { n + 1 });
            assert_eq!(a.analyze(&sine(440.0, 2048, 0.5), 0.1).len(), n);
        }
    }

    #[test]
    fn sample_rate_change_rebuilds_edges() {
        let mut a = analyzer(8);
        a.set_sample_rate(22_050);
        assert!((a.edges()[8] - 11_025.0).abs() < 1.0);
        a.set_sample_rate(1); // clamped, must not panic
        a.analyze(&sine(440.0, 2048, 0.5), 0.1);
    }

    #[test]
    fn fft_size_is_rounded_to_power_of_two() {
        let a = SpectrumAnalyzer::new(SpectrumConfig {
            fft_size: 1000,
            ..SpectrumConfig::default()
        });
        assert_eq!(a.fft_size(), 1024);
        let b = SpectrumAnalyzer::new(SpectrumConfig {
            fft_size: 0,
            ..SpectrumConfig::default()
        });
        assert_eq!(b.fft_size(), 4);
    }

    #[test]
    fn smooth_attacks_fast_and_releases_slowly() {
        let up = smooth(0.0, 1.0, ATTACK_SECS, ATTACK_SECS, RELEASE_SECS);
        assert!((up - (1.0 - (-1.0f32).exp())).abs() < 1e-5);
        let down = smooth(1.0, 0.0, ATTACK_SECS, ATTACK_SECS, RELEASE_SECS);
        assert!(down > 0.85, "release too fast: {down}");
        assert!(up > 0.6);
        assert_eq!(smooth(0.5, 1.0, 0.0, 0.03, 0.3), 0.5);
        assert_eq!(smooth(0.5, 1.0, 0.1, 0.0, 0.3), 1.0);
    }

    #[test]
    fn analyzer_releases_gradually_after_signal_stops() {
        let mut a = analyzer(16);
        let tone = sine(1000.0, 2048, 0.5);
        a.analyze(&tone, 1.0);
        let b = band_of(a.edges(), 1000.0);
        let loud = a.bands()[b];
        assert!(loud > 0.5);
        let silence = vec![0.0; 2048];
        a.analyze(&silence, 0.1);
        let after_one = a.bands()[b];
        assert!(after_one < loud && after_one > loud * 0.5, "{after_one}");
        for _ in 0..100 {
            a.analyze(&silence, 0.1);
        }
        assert!(a.bands()[b] < 0.001);
    }

    #[test]
    fn analyzer_attack_is_faster_than_release() {
        let mut a = analyzer(16);
        let tone = sine(1000.0, 2048, 0.5);
        a.analyze(&tone, 0.05);
        let b = band_of(a.edges(), 1000.0);
        let risen = a.bands()[b];
        assert!(risen > 0.3, "attack too slow: {risen}");
    }

    #[test]
    fn peaks_hold_then_fall_and_never_below_bands() {
        let mut a = analyzer(16);
        let tone = sine(1000.0, 2048, 0.5);
        let silence = vec![0.0; 2048];
        a.analyze(&tone, 1.0);
        let b = band_of(a.edges(), 1000.0);
        let top = a.peaks()[b];
        assert!(top > 0.5);
        // Within the hold time the peak stays put.
        a.analyze(&silence, 0.1);
        assert_eq!(a.peaks()[b], top);
        // After the hold time it falls, but stays >= the band value.
        for _ in 0..10 {
            a.analyze(&silence, 0.1);
            assert!(a.peaks()[b] >= a.bands()[b]);
        }
        assert!(a.peaks()[b] < top);
    }

    #[test]
    fn levels_rms_and_peak() {
        let (rms, peak) = rms_and_peak(&sine(440.0, 44_100, 0.5));
        assert!((rms - 0.5 / 2.0f32.sqrt()).abs() < 1e-3);
        assert!((peak - 0.5).abs() < 1e-3);
        assert_eq!(rms_and_peak(&[]), (0.0, 0.0));
        let (r, p) = rms_and_peak(&[f32::NAN, 0.5]);
        assert!(r.is_finite() && p == 0.5);

        let mut a = analyzer(8);
        a.analyze(&sine(440.0, 2048, 0.5), 0.1);
        assert!((a.peak_level() - 0.5).abs() < 1e-3);
        assert!(a.rms() > 0.3);
    }

    #[test]
    fn reset_zeroes_everything() {
        let mut a = analyzer(8);
        a.analyze(&sine(440.0, 2048, 0.5), 1.0);
        a.reset();
        assert!(a.bands().iter().chain(a.peaks()).all(|&v| v == 0.0));
        assert_eq!(a.rms(), 0.0);
    }

    #[test]
    fn louder_signal_gives_higher_band() {
        let mut a = analyzer(16);
        let b = band_of(a.edges(), 1000.0);
        a.analyze(&sine(1000.0, 2048, 0.01), 1.0);
        let quiet = a.bands()[b];
        a.reset();
        a.analyze(&sine(1000.0, 2048, 0.5), 1.0);
        assert!(quiet > 0.0, "quiet music must still register");
        assert!(a.bands()[b] > quiet);
    }
}
