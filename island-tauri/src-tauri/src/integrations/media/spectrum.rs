//! Pure Rust spectrum analyzer for the Hermes Island music widget
//! (SPEC §8 Phase 6 "Music extras").
//!
//! Turns interleaved PCM audio samples into a small number of smoothed,
//! log-spaced frequency bands for an equalizer visual:
//!
//! * Hann windowing, an iterative radix-2 FFT written in this crate, and a
//!   magnitude normalization so a full-scale sine reads ~0 dB.
//! * Log-spaced band edges between `min_hz` and `max_hz`; each band is the
//!   maximum of its bins' levels in dB, mapped linearly from `floor_db..0`
//!   dB to `0..1`. Every band is guaranteed at least one bin (a narrow low
//!   band borrows its nearest bin).
//! * Exponential per-band smoothing: rising values approach the target with
//!   `attack`, falling ones with `release`.
//!
//! Platform-free: no IO, no network, no threads, no external dependencies,
//! no `unsafe`.

#![forbid(unsafe_code)]

use std::fmt;

/// Configuration for [`Analyzer`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandConfig {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// FFT size; a power of two in 256..=16384.
    pub fft_size: usize,
    /// Number of output frequency bands (1..=64).
    pub bands: usize,
    /// Lowest band edge in Hz (> 0).
    pub min_hz: f32,
    /// Highest band edge in Hz (<= sample_rate / 2).
    pub max_hz: f32,
    /// Rise smoothing coefficient in (0, 1].
    pub attack: f32,
    /// Fall smoothing coefficient in (0, 1].
    pub release: f32,
    /// Noise floor in dB (< 0); the bottom of the linear 0..1 mapping.
    pub floor_db: f32,
}

impl Default for BandConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            fft_size: 2048,
            bands: 16,
            min_hz: 40.0,
            max_hz: 16_000.0,
            attack: 0.6,
            release: 0.15,
            floor_db: -70.0,
        }
    }
}

/// Errors reported by [`BandConfig::validate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// `fft_size` is not a power of two in 256..=16384.
    BadFftSize,
    /// `bands` is outside 1..=64.
    BadBands,
    /// The pair `min_hz`/`max_hz` does not satisfy 0 < min < max <= sample_rate/2.
    BadFrequencyRange,
    /// `attack`/`release` is outside (0, 1].
    BadSmoothing,
    /// `floor_db` is not negative.
    BadFloor,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::BadFftSize => {
                write!(f, "fft_size must be a power of two in 256..=16384")
            }
            ConfigError::BadBands => write!(f, "bands must be in 1..=64"),
            ConfigError::BadFrequencyRange => {
                write!(f, "require 0 < min_hz < max_hz <= sample_rate / 2")
            }
            ConfigError::BadSmoothing => write!(f, "attack and release must be in (0, 1]"),
            ConfigError::BadFloor => write!(f, "floor_db must be negative"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl BandConfig {
    /// Validate this configuration against the documented invariants.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.fft_size.is_power_of_two() || !(256..=16384).contains(&self.fft_size) {
            return Err(ConfigError::BadFftSize);
        }
        if !(1..=64).contains(&self.bands) {
            return Err(ConfigError::BadBands);
        }
        let nyquist = self.sample_rate as f32 / 2.0;
        if !(self.min_hz > 0.0 && self.max_hz > self.min_hz && self.max_hz <= nyquist) {
            return Err(ConfigError::BadFrequencyRange);
        }
        if !(self.attack > 0.0 && self.attack <= 1.0 && self.release > 0.0 && self.release <= 1.0) {
            return Err(ConfigError::BadSmoothing);
        }
        if self.floor_db.partial_cmp(&0.0) != Some(std::cmp::Ordering::Less) {
            return Err(ConfigError::BadFloor);
        }
        Ok(())
    }
}

/// Streaming spectrum analyzer. Buffers incoming mono samples and, once at
/// least `fft_size` samples have arrived, produces the smoothed band levels
/// for the newest `fft_size` samples.
///
/// All buffers are allocated in [`Analyzer::new`]; [`Analyzer::bands`] never
/// allocates after construction (pinned by capacity and pointer-stability
/// tests in `src/lib.rs`).
#[derive(Debug)]
pub struct Analyzer {
    config: BandConfig,
    // Ring buffer holding the latest `fft_size` mono samples.
    buf: Vec<f32>,
    write: usize,
    filled: usize,
    total: u64,
    // Precomputed periodic Hann window.
    hann: Vec<f32>,
    // FFT working buffers (also hold the windowed time samples pre-FFT).
    real: Vec<f32>,
    imag: Vec<f32>,
    // Magnitudes for bins 0..=fft_size/2.
    mag: Vec<f32>,
    // Band -> bin mapping (flat bins plus offsets per band).
    bin_off: Vec<usize>,
    bin_flat: Vec<usize>,
    // Smoothed level per band, in dB.
    cur_db: Vec<f32>,
    // Final [0, 1] output, returned from `bands`.
    out: Vec<f32>,
}

impl Analyzer {
    /// Create an analyzer, validating `config` and preallocating all buffers.
    pub fn new(config: BandConfig) -> Result<Self, ConfigError> {
        config.validate()?;
        let n = config.fft_size;
        let b = config.bands;
        let half = n / 2;

        let mut hann = vec![0.0f32; n];
        let pi2 = 2.0 * std::f32::consts::PI;
        for (i, w) in hann.iter_mut().enumerate() {
            *w = 0.5 * (1.0 - (pi2 * i as f32 / n as f32).cos());
        }

        let (bin_off, bin_flat) = build_band_map(&config);

        Ok(Self {
            config,
            buf: vec![0.0; n],
            write: 0,
            filled: 0,
            total: 0,
            hann,
            real: vec![0.0; n],
            imag: vec![0.0; n],
            mag: vec![0.0; half + 1],
            bin_off,
            bin_flat,
            cur_db: vec![config.floor_db; b],
            out: vec![0.0; b],
        })
    }

    /// Buffer `interleaved` audio: downmix to mono by averaging `channels`
    /// per frame, then append. Non-finite samples contribute 0.0. An empty
    /// slice or `channels == 0` is a no-op.
    pub fn push(&mut self, interleaved: &[f32], channels: u16) {
        if interleaved.is_empty() || channels == 0 {
            return;
        }
        let ch = channels as usize;
        let n = self.config.fft_size;
        let mut i = 0usize;
        while interleaved.len() - i >= ch {
            let mut sum = 0.0f32;
            for c in 0..ch {
                let s = interleaved[i + c];
                sum += if s.is_finite() { s } else { 0.0 };
            }
            self.buf[self.write] = sum / ch as f32;
            self.write += 1;
            if self.write == n {
                self.write = 0;
            }
            if self.filled < n {
                self.filled += 1;
            }
            self.total += 1;
            i += ch;
        }
    }

    /// Analyze the newest `fft_size` samples and return smoothed band levels
    /// in [0, 1]. Returns `None` until `fft_size` samples have been pushed.
    pub fn bands(&mut self) -> Option<&[f32]> {
        let n = self.config.fft_size;
        let b = self.config.bands;
        if self.total < n as u64 {
            return None;
        }
        // Oldest of the newest n samples sits at `write` once the ring is full.
        let start = self.write;
        for i in 0..n {
            self.real[i] = self.buf[(start + i) % n] * self.hann[i];
        }
        for v in self.imag.iter_mut() {
            *v = 0.0;
        }
        fft(&mut self.real, &mut self.imag);

        let half = n / 2;
        for k in 0..=half {
            let r = self.real[k];
            let im = self.imag[k];
            self.mag[k] = (r * r + im * im).sqrt();
        }

        let norm = n as f32 / 4.0;
        let floor = self.config.floor_db;
        for band in 0..b {
            let lo = self.bin_off[band];
            let hi = self.bin_off[band + 1];
            let mut max_mag = 0.0f32;
            for &k in &self.bin_flat[lo..hi] {
                let m = self.mag[k];
                if m > max_mag {
                    max_mag = m;
                }
            }
            let norm_mag = max_mag / norm;
            let target_db = if norm_mag > 0.0 {
                (20.0 * norm_mag.log10()).max(floor)
            } else {
                floor
            };
            let cur = self.cur_db[band];
            let coef = if target_db > cur {
                self.config.attack
            } else {
                self.config.release
            };
            let smoothed = cur + coef * (target_db - cur);
            self.cur_db[band] = smoothed;
            let scaled = (smoothed - floor) / (-floor);
            self.out[band] = scaled.clamp(0.0, 1.0);
        }
        Some(&self.out)
    }
}

/// Map every band to its bins: log-spaced edges between `min_hz` and
/// `max_hz`, bins whose center frequency falls inside the band, and a
/// borrow-nearest-bin fallback when a band would otherwise own no bin.
fn build_band_map(config: &BandConfig) -> (Vec<usize>, Vec<usize>) {
    let n = config.fft_size;
    let b = config.bands;
    let sr = config.sample_rate as f32;
    let half = n / 2;
    let bin_hz = sr / n as f32;

    let ratio = (config.max_hz / config.min_hz).powf(1.0 / b as f32);
    let mut edges = Vec::with_capacity(b + 1);
    for i in 0..=b {
        edges.push(config.min_hz * ratio.powi(i as i32));
    }
    edges[b] = config.max_hz;

    let freq = |k: usize| k as f32 * bin_hz;
    let mut off = Vec::with_capacity(b + 1);
    let mut flat: Vec<usize> = Vec::new();
    off.push(0);
    for i in 0..b {
        let lo = edges[i];
        let hi = edges[i + 1];
        let mut picked: Vec<usize> = (0..=half)
            .filter(|&k| freq(k) >= lo && freq(k) < hi)
            .collect();
        if picked.is_empty() {
            // Borrow the bin nearest the band's geometric center.
            let center = (lo * hi).sqrt();
            let mut best = 0usize;
            let mut best_d = f32::INFINITY;
            for k in 0..=half {
                let d = (freq(k) - center).abs();
                if d < best_d {
                    best_d = d;
                    best = k;
                }
            }
            picked.push(best);
        }
        flat.extend_from_slice(&picked);
        off.push(flat.len());
    }
    (off, flat)
}

/// Iterative radix-2 Cooley-Tukey FFT in place over `real`/`imag`.
fn fft(real: &mut [f32], imag: &mut [f32]) {
    let n = real.len();
    debug_assert!(n.is_power_of_two());
    debug_assert_eq!(real.len(), imag.len());

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while (j & bit) != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
    }

    // Butterfly stages.
    let mut len = 2usize;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let wlen_r = ang.cos();
        let wlen_i = ang.sin();
        let half = len >> 1;
        let mut group = 0usize;
        while group < n {
            let mut wr = 1.0f32;
            let mut wi = 0.0f32;
            for k in 0..half {
                let u = group + k;
                let v = u + half;
                let vr = real[v] * wr - imag[v] * wi;
                let vi = real[v] * wi + imag[v] * wr;
                let ur = real[u];
                let ui = imag[u];
                real[u] = ur + vr;
                imag[u] = ui + vi;
                real[v] = ur - vr;
                imag[v] = ui - vi;
                let nwr = wr * wlen_r - wi * wlen_i;
                let nwi = wr * wlen_i + wi * wlen_r;
                wr = nwr;
                wi = nwi;
            }
            group += len;
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_matches_direct_dft() {
        let cfg = BandConfig {
            fft_size: 256,
            ..BandConfig::default()
        };
        let n = cfg.fft_size;
        let mut sig = vec![0.0f32; n];
        let mut seed = 0x1234_5678u32;
        for (i, s) in sig.iter_mut().enumerate() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *s = ((seed >> 8) as f32 / 16_777_216.0) - 0.5;
            *s += 0.5 * (2.0 * std::f32::consts::PI * 37.0 * i as f32 / n as f32).sin();
        }
        let mut a = Analyzer::new(cfg).unwrap();
        a.push(&sig, 1);
        let _ = a.bands();

        let pi2 = 2.0 * std::f32::consts::PI;
        let mut ref_real = vec![0.0f32; n];
        let mut ref_imag = vec![0.0f32; n];
        for k in 0..n {
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for (t, &x) in sig.iter().enumerate() {
                let w = 0.5 * (1.0 - (pi2 * t as f32 / n as f32).cos());
                let ang = pi2 * k as f32 * t as f32 / n as f32;
                re += x * w * ang.cos();
                im -= x * w * ang.sin();
            }
            ref_real[k] = re;
            ref_imag[k] = im;
        }
        for k in 0..n {
            let scale = ref_real[k].abs().max(ref_imag[k].abs()).max(1e-6);
            assert!(
                (a.real[k] - ref_real[k]).abs() <= 1e-3 * scale + 1e-4,
                "real bin {}: got {}, want {}",
                k,
                a.real[k],
                ref_real[k]
            );
            assert!(
                (a.imag[k] - ref_imag[k]).abs() <= 1e-3 * scale + 1e-4,
                "imag bin {}: got {}, want {}",
                k,
                a.imag[k],
                ref_imag[k]
            );
        }
    }

    #[test]
    fn full_scale_on_bin_sine_normalizes_to_zero_db() {
        let cfg = BandConfig {
            fft_size: 256,
            ..BandConfig::default()
        };
        let n = cfg.fft_size;
        let k0 = 32usize;
        let sig: Vec<f32> = (0..n)
            .map(|t| (2.0 * std::f32::consts::PI * k0 as f32 * t as f32 / n as f32).sin())
            .collect();
        let mut a = Analyzer::new(cfg).unwrap();
        a.push(&sig, 1);
        let _ = a.bands();
        let norm = n as f32 / 4.0;
        let m = (a.real[k0] * a.real[k0] + a.imag[k0] * a.imag[k0]).sqrt() / norm;
        assert!((m - 1.0).abs() < 1e-3, "normalized magnitude = {}", m);
    }

    #[test]
    fn ring_buffer_keeps_newest_n_samples() {
        let cfg = BandConfig {
            fft_size: 256,
            ..BandConfig::default()
        };
        let n = cfg.fft_size;
        let mut a = Analyzer::new(cfg).unwrap();
        let samples: Vec<f32> = (0..n + 4).map(|i| i as f32).collect();
        a.push(&samples, 1);
        assert_eq!(a.filled, n);
        assert_eq!(a.total, (n + 4) as u64);
        for i in 0..n {
            let got = a.buf[(a.write + i) % n];
            let want = (4 + i) as f32;
            assert!(
                (got - want).abs() < 1e-6,
                "ring slot {}: got {}, want {}",
                i,
                got,
                want
            );
        }
    }

    #[test]
    fn buffer_capacities_preallocated() {
        let cfg = BandConfig::default();
        let a = Analyzer::new(cfg).unwrap();
        let n = cfg.fft_size;
        let b = cfg.bands;
        assert!(a.buf.capacity() >= n);
        assert!(a.hann.capacity() >= n);
        assert!(a.real.capacity() >= n);
        assert!(a.imag.capacity() >= n);
        assert!(a.mag.capacity() > n / 2);
        assert!(a.cur_db.capacity() >= b);
        assert!(a.out.capacity() >= b);
        assert!(a.bin_off.capacity() > b);
        assert!(a.bin_flat.capacity() >= a.bin_flat.len());
    }

    #[test]
    fn bands_does_not_reallocate_after_construction() {
        let cfg = BandConfig::default();
        let n = cfg.fft_size;
        let mut a = Analyzer::new(cfg).unwrap();
        let sig = (0..n)
            .map(|t| {
                (2.0 * std::f32::consts::PI * 1000.0 * t as f32 / cfg.sample_rate as f32).sin()
            })
            .collect::<Vec<f32>>();
        a.push(&sig, 1);

        let before_out = a.bands().unwrap().as_ptr() as usize;
        let before = (
            a.buf.as_ptr() as usize,
            a.real.as_ptr() as usize,
            a.imag.as_ptr() as usize,
            a.mag.as_ptr() as usize,
            a.cur_db.as_ptr() as usize,
            a.out.as_ptr() as usize,
        );

        for _ in 0..200 {
            let _ = a.bands();
        }

        let after_out = a.bands().unwrap().as_ptr() as usize;
        let after = (
            a.buf.as_ptr() as usize,
            a.real.as_ptr() as usize,
            a.imag.as_ptr() as usize,
            a.mag.as_ptr() as usize,
            a.cur_db.as_ptr() as usize,
            a.out.as_ptr() as usize,
        );
        assert_eq!(before, after);
        assert_eq!(before_out, after_out);
    }
}
