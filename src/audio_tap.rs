//! Audio tap: lets the UI look at what the player is decoding without
//! touching the audio itself.
//!
//! Data flow:
//!
//! ```text
//! rodio Decoder -> TapSource -> (unchanged samples) -> Sink -> speakers
//!                      |
//!                      | every BLOCK_SAMPLES samples, one try_lock
//!                      v
//!               TapHandle ring (last RING_FRAMES mono samples + levels)
//!                      |
//!                      | snapshot_delayed(n, delay)  (UI thread)
//!                      v
//!               SpectrumAnalyzer (operations/spectrum.rs)
//! ```
//!
//! Real-time rules for the audio thread (the one calling
//! [`TapSource::next`]): samples are forwarded untouched, no lock is taken
//! per sample, the shared state is only ever entered with `try_lock` (a
//! contended or poisoned lock just drops that block), and nothing here can
//! panic or allocate in the steady state.
//!
//! Latency: the sink buffers audio before it reaches the speakers, so the
//! newest tapped sample is heard slightly later. The UI should analyze a
//! slightly older window, e.g. `snapshot_delayed(fft_size, delay)` with
//! `delay` of roughly 50-100 ms worth of samples ([`SUGGESTED_DELAY_MS`]).
//! It is deliberately not tuned yet.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::Source;

/// Interleaved samples gathered locally before one push to the shared
/// ring (256 stereo frames, ~6 ms at 44.1 kHz).
pub const BLOCK_SAMPLES: usize = 512;
/// Mono samples kept in the ring: 4x the default FFT size, which leaves
/// room for the FFT window plus a ~100 ms display delay at 48 kHz.
pub const RING_FRAMES: usize = 8192;
/// Rough size of the audio-output latency the UI should compensate for.
pub const SUGGESTED_DELAY_MS: u32 = 75;

/// Per-channel levels accumulated since the previous [`TapHandle::levels`]
/// call. Mono sources report the same value for left and right. All zero
/// when no new audio arrived (paused or stopped).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Levels {
    /// Absolute peak of the left channel, `0.0..=1.0`.
    pub left_peak: f32,
    /// RMS of the left channel.
    pub left_rms: f32,
    /// Absolute peak of the right channel.
    pub right_peak: f32,
    /// RMS of the right channel.
    pub right_rms: f32,
}

/// Level statistics of one block, computed outside the lock.
#[derive(Debug, Clone, Copy, Default)]
struct BlockStats {
    peak_l: f32,
    peak_r: f32,
    sumsq_l: f64,
    sumsq_r: f64,
    frames: u64,
}

struct TapState {
    ring: Vec<f32>,
    /// Index where the next mono sample is written.
    write: usize,
    /// Number of valid samples in the ring (<= capacity).
    filled: usize,
    sample_rate: u32,
    channels: u16,
    /// Bumped by `clear`; blocks from sources created before it are dropped.
    epoch: u64,
    frames_written: u64,
    acc: BlockStats,
}

impl TapState {
    fn new(capacity: usize) -> Self {
        Self {
            ring: vec![0.0; capacity.max(1)],
            write: 0,
            filled: 0,
            sample_rate: 0,
            channels: 0,
            epoch: 0,
            frames_written: 0,
            acc: BlockStats::default(),
        }
    }

    fn push(&mut self, mono: &[f32], stats: &BlockStats, sample_rate: u32, channels: u16) {
        let cap = self.ring.len();
        let mono = if mono.len() > cap {
            &mono[mono.len() - cap..]
        } else {
            mono
        };
        for &s in mono {
            self.ring[self.write] = s;
            self.write = (self.write + 1) % cap;
        }
        self.filled = (self.filled + mono.len()).min(cap);
        self.sample_rate = sample_rate;
        self.channels = channels;
        self.frames_written += stats.frames;
        self.acc.peak_l = self.acc.peak_l.max(stats.peak_l);
        self.acc.peak_r = self.acc.peak_r.max(stats.peak_r);
        self.acc.sumsq_l += stats.sumsq_l;
        self.acc.sumsq_r += stats.sumsq_r;
        self.acc.frames += stats.frames;
    }
}

/// Shared handle to the tap state; cloning shares the same ring.
#[derive(Clone)]
pub struct TapHandle(Arc<Mutex<TapState>>);

impl Default for TapHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl TapHandle {
    /// Creates an empty tap with [`RING_FRAMES`] of capacity.
    pub fn new() -> Self {
        Self::with_capacity(RING_FRAMES)
    }

    /// Creates an empty tap holding the last `capacity` mono samples.
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Arc::new(Mutex::new(TapState::new(capacity))))
    }

    /// Drops all samples and levels (new track, stop). Sources created
    /// before this call stop contributing, so a dying old source cannot
    /// leave stale samples behind.
    pub fn clear(&self) {
        if let Ok(mut s) = self.0.lock() {
            s.write = 0;
            s.filled = 0;
            s.sample_rate = 0;
            s.channels = 0;
            s.frames_written = 0;
            s.acc = BlockStats::default();
            s.epoch = s.epoch.wrapping_add(1);
        }
    }

    fn epoch(&self) -> u64 {
        self.0.lock().map(|s| s.epoch).unwrap_or(0)
    }

    /// Audio-thread entry point: never blocks. Returns `false` when the
    /// block was dropped (contended/poisoned lock or stale epoch).
    fn try_push(
        &self,
        epoch: u64,
        mono: &[f32],
        stats: &BlockStats,
        sample_rate: u32,
        channels: u16,
    ) -> bool {
        match self.0.try_lock() {
            Ok(mut s) if s.epoch == epoch => {
                s.push(mono, stats, sample_rate, channels);
                true
            }
            _ => false,
        }
    }

    /// Copy of the newest `n` mono samples, oldest first, zero-padded at
    /// the front when fewer are available.
    #[cfg(test)]
    pub fn snapshot_latest(&self, n: usize) -> Vec<f32> {
        self.snapshot_delayed(n, 0)
    }

    /// Like [`TapHandle::snapshot_latest`] but the window ends `delay`
    /// samples before the newest one, to line the display up with the
    /// audio that is actually being heard (see the module docs). Missing
    /// samples (window reaching before the start or past the end of the
    /// data) are zeros.
    pub fn snapshot_delayed(&self, n: usize, delay: usize) -> Vec<f32> {
        let mut out = vec![0.0; n];
        let Ok(s) = self.0.lock() else {
            return out;
        };
        let cap = s.ring.len();
        // Logical index 0 is the oldest valid sample.
        let start = s.filled as i64 - delay as i64 - n as i64;
        let oldest = (s.write + cap - s.filled) % cap;
        for (i, o) in out.iter_mut().enumerate() {
            let idx = start + i as i64;
            if idx >= 0 && idx < s.filled as i64 {
                *o = s.ring[(oldest + idx as usize) % cap];
            }
        }
        out
    }

    /// Levels since the previous call (resets the accumulator).
    pub fn levels(&self) -> Levels {
        let Ok(mut s) = self.0.lock() else {
            return Levels::default();
        };
        let acc = std::mem::take(&mut s.acc);
        if acc.frames == 0 {
            return Levels::default();
        }
        let f = acc.frames as f64;
        Levels {
            left_peak: acc.peak_l,
            left_rms: (acc.sumsq_l / f).sqrt() as f32,
            right_peak: acc.peak_r,
            right_rms: (acc.sumsq_r / f).sqrt() as f32,
        }
    }

    /// Sample rate of the tapped source (0 when nothing was tapped yet).
    pub fn sample_rate(&self) -> u32 {
        self.0.lock().map(|s| s.sample_rate).unwrap_or(0)
    }

    /// Channel count of the tapped source (0 when nothing was tapped yet).
    #[cfg(test)]
    pub fn channels(&self) -> u16 {
        self.0.lock().map(|s| s.channels).unwrap_or(0)
    }

    /// Frames tapped since the last `clear`. A value that stops growing
    /// tells the UI that playback is paused or has ended.
    pub fn frames_written(&self) -> u64 {
        self.0.lock().map(|s| s.frames_written).unwrap_or(0)
    }

    /// Number of samples per millisecond-delay helper: converts a delay in
    /// milliseconds to mono samples at `sample_rate`.
    pub fn delay_samples(sample_rate: u32, delay_ms: u32) -> usize {
        (sample_rate as u64 * delay_ms as u64 / 1000) as usize
    }
}

/// `rodio::Source` adapter that forwards every sample of `S` unchanged and
/// copies it (downmixed to mono) into a [`TapHandle`].
///
/// `Item` stays `i16` (what the symphonia `Decoder` yields) so the sink
/// receives exactly the same values as without the tap.
pub struct TapSource<S: Source<Item = i16>> {
    inner: S,
    tap: TapHandle,
    epoch: u64,
    block_samples: usize,
    /// Interleaved samples of the block being gathered, as `f32`.
    block: Vec<f32>,
    /// Scratch for the mono downmix, reused to avoid allocating per block.
    mono: Vec<f32>,
}

impl<S: Source<Item = i16>> TapSource<S> {
    /// Wraps `inner`, tapping into `tap` with [`BLOCK_SAMPLES`] blocks.
    pub fn new(inner: S, tap: TapHandle) -> Self {
        Self::with_block_samples(inner, tap, BLOCK_SAMPLES)
    }

    /// Like [`TapSource::new`] with an explicit block size (min 1).
    pub fn with_block_samples(inner: S, tap: TapHandle, block_samples: usize) -> Self {
        let block_samples = block_samples.max(1);
        let epoch = tap.epoch();
        Self {
            inner,
            tap,
            epoch,
            block_samples,
            block: Vec::with_capacity(block_samples + 8),
            mono: Vec::with_capacity(block_samples / 2 + 8),
        }
    }

    /// Pushes the whole frames in `block` (if any) and empties it. A
    /// trailing partial frame is kept so channels never get misaligned.
    fn flush(&mut self) {
        let ch = self.inner.channels().max(1) as usize;
        let whole = self.block.len() - self.block.len() % ch;
        if whole == 0 {
            return;
        }
        self.mono.clear();
        let mut stats = BlockStats::default();
        for frame in self.block[..whole].chunks_exact(ch) {
            let sum: f32 = frame.iter().sum();
            self.mono.push(sum / ch as f32);
            let l = frame[0];
            let r = if ch > 1 { frame[1] } else { l };
            stats.peak_l = stats.peak_l.max(l.abs());
            stats.peak_r = stats.peak_r.max(r.abs());
            stats.sumsq_l += (l as f64) * (l as f64);
            stats.sumsq_r += (r as f64) * (r as f64);
            stats.frames += 1;
        }
        // Contended lock or stale epoch: the block is simply dropped.
        let _ = self.tap.try_push(
            self.epoch,
            &self.mono,
            &stats,
            self.inner.sample_rate(),
            ch.min(u16::MAX as usize) as u16,
        );
        self.block.drain(..whole);
    }
}

impl<S: Source<Item = i16>> Iterator for TapSource<S> {
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        match self.inner.next() {
            Some(s) => {
                self.block.push(s as f32 / 32768.0);
                if self.block.len() >= self.block_samples {
                    self.flush();
                }
                Some(s)
            }
            None => {
                self.flush();
                None
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source<Item = i16>> Source for TapSource<S> {
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeSource {
        data: std::vec::IntoIter<i16>,
        channels: u16,
        rate: u32,
    }

    fn fake(data: Vec<i16>, channels: u16, rate: u32) -> FakeSource {
        FakeSource {
            data: data.into_iter(),
            channels,
            rate,
        }
    }

    impl Iterator for FakeSource {
        type Item = i16;
        fn next(&mut self) -> Option<i16> {
            self.data.next()
        }
    }

    impl Source for FakeSource {
        fn current_frame_len(&self) -> Option<usize> {
            Some(123)
        }
        fn channels(&self) -> u16 {
            self.channels
        }
        fn sample_rate(&self) -> u32 {
            self.rate
        }
        fn total_duration(&self) -> Option<Duration> {
            Some(Duration::from_secs(7))
        }
    }

    fn ramp(n: usize) -> Vec<i16> {
        (0..n).map(|i| (i as i16).wrapping_mul(37)).collect()
    }

    #[test]
    fn samples_pass_through_unchanged_and_in_order() {
        let data = ramp(2000);
        let tap = TapHandle::new();
        let out: Vec<i16> = TapSource::new(fake(data.clone(), 2, 44_100), tap).collect();
        assert_eq!(out, data);
    }

    #[test]
    fn source_metadata_is_delegated() {
        let src = TapSource::new(fake(vec![], 2, 48_000), TapHandle::new());
        assert_eq!(src.channels(), 2);
        assert_eq!(src.sample_rate(), 48_000);
        assert_eq!(src.current_frame_len(), Some(123));
        assert_eq!(src.total_duration(), Some(Duration::from_secs(7)));
    }

    #[test]
    fn ring_holds_newest_samples() {
        // Mono, tiny ring: only the newest 8 samples survive.
        let data: Vec<i16> = (1..=100).map(|i| i * 100).collect();
        let tap = TapHandle::with_capacity(8);
        let src = TapSource::with_block_samples(fake(data.clone(), 1, 44_100), tap.clone(), 16);
        assert_eq!(src.count(), 100);
        let snap = tap.snapshot_latest(8);
        let expected: Vec<f32> = data[92..].iter().map(|&s| s as f32 / 32768.0).collect();
        assert_eq!(snap, expected);
        assert_eq!(tap.sample_rate(), 44_100);
        assert_eq!(tap.channels(), 1);
        assert_eq!(tap.frames_written(), 100);
    }

    #[test]
    fn snapshot_zero_pads_at_the_front() {
        let tap = TapHandle::new();
        let _: Vec<i16> =
            TapSource::with_block_samples(fake(vec![16384; 4], 1, 44_100), tap.clone(), 2)
                .collect();
        let snap = tap.snapshot_latest(6);
        assert_eq!(snap, vec![0.0, 0.0, 0.5, 0.5, 0.5, 0.5]);
        assert_eq!(TapHandle::new().snapshot_latest(3), vec![0.0; 3]);
        assert!(TapHandle::new().snapshot_latest(0).is_empty());
    }

    #[test]
    fn snapshot_delayed_shifts_the_window_back() {
        let data: Vec<i16> = (0..10).map(|i| i * 1000).collect();
        let tap = TapHandle::new();
        let _: Vec<i16> =
            TapSource::with_block_samples(fake(data.clone(), 1, 44_100), tap.clone(), 5).collect();
        let f = |i: usize| data[i] as f32 / 32768.0;
        // Window of 3 ending 2 samples before the newest (index 9).
        assert_eq!(tap.snapshot_delayed(3, 2), vec![f(5), f(6), f(7)]);
        assert_eq!(tap.snapshot_delayed(3, 0), vec![f(7), f(8), f(9)]);
        // Delay reaching past the oldest sample pads with zeros.
        assert_eq!(tap.snapshot_delayed(3, 9), vec![0.0, 0.0, f(0)]);
        // Delay larger than the data: silence, no panic.
        assert_eq!(tap.snapshot_delayed(3, 100), vec![0.0; 3]);
        assert_eq!(TapHandle::delay_samples(48_000, 75), 3600);
    }

    #[test]
    fn stereo_is_downmixed_to_mono_with_channel_levels() {
        // L = +0.5, R = -0.25 for every frame.
        let mut data = Vec::new();
        for _ in 0..64 {
            data.extend_from_slice(&[16384, -8192]);
        }
        let tap = TapHandle::new();
        let out: Vec<i16> =
            TapSource::with_block_samples(fake(data.clone(), 2, 44_100), tap.clone(), 32).collect();
        assert_eq!(out, data);
        assert_eq!(tap.channels(), 2);
        assert_eq!(tap.frames_written(), 64);
        let snap = tap.snapshot_latest(64);
        assert!(snap.iter().all(|&v| (v - 0.125).abs() < 1e-6));
        let lv = tap.levels();
        assert!((lv.left_peak - 0.5).abs() < 1e-6);
        assert!((lv.right_peak - 0.25).abs() < 1e-6);
        assert!((lv.left_rms - 0.5).abs() < 1e-6);
        assert!((lv.right_rms - 0.25).abs() < 1e-6);
        // Accumulator was reset: nothing new, all zeros.
        assert_eq!(tap.levels(), Levels::default());
    }

    #[test]
    fn mono_levels_report_the_same_value_for_both_sides() {
        let tap = TapHandle::new();
        let _: Vec<i16> =
            TapSource::with_block_samples(fake(vec![16384; 10], 1, 44_100), tap.clone(), 4)
                .collect();
        let lv = tap.levels();
        assert_eq!(lv.left_peak, lv.right_peak);
        assert_eq!(lv.left_rms, lv.right_rms);
    }

    #[test]
    fn partial_frames_are_never_split_across_blocks() {
        // Block size 3 with stereo: flushes must stay frame-aligned, so the
        // mono ring is exactly the per-frame averages.
        let mut data = Vec::new();
        for i in 0..20i16 {
            data.extend_from_slice(&[i * 1000, i * 1000]);
        }
        let tap = TapHandle::new();
        let _: Vec<i16> =
            TapSource::with_block_samples(fake(data, 2, 44_100), tap.clone(), 3).collect();
        let snap = tap.snapshot_latest(20);
        for (i, v) in snap.iter().enumerate() {
            assert!(
                (v - (i as f32 * 1000.0 / 32768.0)).abs() < 1e-6,
                "frame {i}"
            );
        }
    }

    #[test]
    fn contended_lock_drops_blocks_but_never_blocks_or_alters_audio() {
        let data = ramp(1000);
        let tap = TapHandle::with_capacity(64);
        let src = TapSource::with_block_samples(fake(data.clone(), 1, 44_100), tap.clone(), 16);
        let guard = tap.0.lock().unwrap();
        // If the audio path waited on the lock this would deadlock.
        let out: Vec<i16> = src.collect();
        drop(guard);
        assert_eq!(out, data);
        assert_eq!(tap.frames_written(), 0);
        assert_eq!(tap.snapshot_latest(4), vec![0.0; 4]);
    }

    #[test]
    fn poisoned_lock_is_ignored_silently() {
        let tap = TapHandle::new();
        let t2 = tap.clone();
        let _ = std::thread::spawn(move || {
            let _g = t2.0.lock().unwrap();
            panic!("poison the tap");
        })
        .join();
        let data = ramp(100);
        let out: Vec<i16> =
            TapSource::with_block_samples(fake(data.clone(), 1, 44_100), tap.clone(), 8).collect();
        assert_eq!(out, data);
        assert_eq!(tap.snapshot_latest(4), vec![0.0; 4]);
        assert_eq!(tap.levels(), Levels::default());
        assert_eq!(tap.sample_rate(), 0);
        tap.clear();
    }

    #[test]
    fn clear_empties_ring_and_levels() {
        let tap = TapHandle::new();
        let _: Vec<i16> =
            TapSource::with_block_samples(fake(vec![16384; 32], 2, 44_100), tap.clone(), 8)
                .collect();
        assert!(tap.frames_written() > 0);
        tap.clear();
        assert_eq!(tap.snapshot_latest(8), vec![0.0; 8]);
        assert_eq!(tap.levels(), Levels::default());
        assert_eq!(tap.frames_written(), 0);
        assert_eq!(tap.sample_rate(), 0);
    }

    #[test]
    fn sources_created_before_clear_stop_contributing() {
        let tap = TapHandle::new();
        let old = TapSource::with_block_samples(fake(vec![16384; 32], 1, 44_100), tap.clone(), 4);
        tap.clear();
        let new = TapSource::with_block_samples(fake(vec![-16384; 8], 1, 44_100), tap.clone(), 4);
        let _: Vec<i16> = old.collect();
        assert_eq!(tap.frames_written(), 0, "stale source must not push");
        let _: Vec<i16> = new.collect();
        assert_eq!(tap.snapshot_latest(2), vec![-0.5, -0.5]);
    }

    #[test]
    fn trailing_partial_block_is_flushed_at_end() {
        let tap = TapHandle::new();
        let _: Vec<i16> =
            TapSource::with_block_samples(fake(vec![8192; 5], 1, 44_100), tap.clone(), 512)
                .collect();
        assert_eq!(tap.frames_written(), 5);
    }
}
