//! Behavior tests for `spectrum-bands`, exercised through the public API.

use hermes_island_lib::integrations::media::spectrum::{Analyzer, BandConfig};

fn render_sine(sample_rate: u32, freq: f32, amp: f32, n: usize) -> Vec<f32> {
    let sr = sample_rate as f32;
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32;
        v.push(amp * (2.0 * std::f32::consts::PI * freq * t / sr).sin());
    }
    v
}

fn settle(a: &mut Analyzer, its: usize) {
    for _ in 0..its {
        let _ = a.bands();
    }
}

fn band_peak(values: &[f32]) -> usize {
    let mut best = 0usize;
    let mut best_v = f32::NEG_INFINITY;
    for (i, &v) in values.iter().enumerate() {
        if v > best_v {
            best_v = v;
            best = i;
        }
    }
    best
}

fn expected_band(cfg: &BandConfig, hz: f32) -> usize {
    let ratio = (cfg.max_hz / cfg.min_hz).powf(1.0 / cfg.bands as f32);
    let mut e = cfg.min_hz;
    for b in 0..cfg.bands {
        let hi = e * ratio;
        if hz >= e && hz < hi {
            return b;
        }
        e = hi;
    }
    cfg.bands - 1
}

#[test]
fn default_config_values_are_valid() {
    let cfg = BandConfig::default();
    assert!(cfg.validate().is_ok());
    assert_eq!(cfg.sample_rate, 48_000);
    assert_eq!(cfg.fft_size, 2048);
    assert_eq!(cfg.bands, 16);
    assert_eq!(cfg.min_hz, 40.0);
    assert_eq!(cfg.max_hz, 16_000.0);
    assert_eq!(cfg.attack, 0.6);
    assert_eq!(cfg.release, 0.15);
    assert_eq!(cfg.floor_db, -70.0);
}

#[test]
fn invalid_configs_are_rejected() {
    let base = BandConfig::default();
    let bad = [
        BandConfig {
            fft_size: 0,
            ..base
        },
        BandConfig {
            fft_size: 255,
            ..base
        },
        BandConfig {
            fft_size: 2047,
            ..base
        },
        BandConfig {
            fft_size: 32768,
            ..base
        },
        BandConfig { bands: 0, ..base },
        BandConfig { bands: 65, ..base },
        BandConfig {
            min_hz: 0.0,
            ..base
        },
        BandConfig {
            min_hz: 100.0,
            max_hz: 50.0,
            ..base
        },
        BandConfig {
            max_hz: 24_001.0,
            ..base
        },
        BandConfig {
            attack: 0.0,
            ..base
        },
        BandConfig {
            attack: 1.5,
            ..base
        },
        BandConfig {
            release: 0.0,
            ..base
        },
        BandConfig {
            release: 2.0,
            ..base
        },
        BandConfig {
            floor_db: 0.0,
            ..base
        },
        BandConfig {
            floor_db: 5.0,
            ..base
        },
        BandConfig {
            floor_db: f32::NAN,
            ..base
        },
    ];
    for c in bad {
        assert!(c.validate().is_err(), "should reject {:?}", c);
    }
    let good = [
        BandConfig {
            fft_size: 256,
            ..base
        },
        BandConfig {
            fft_size: 16384,
            ..base
        },
        BandConfig { bands: 1, ..base },
        BandConfig { bands: 64, ..base },
        BandConfig {
            attack: 1.0,
            ..base
        },
        BandConfig {
            release: 1.0,
            ..base
        },
        BandConfig {
            floor_db: -0.001,
            ..base
        },
        BandConfig {
            min_hz: 0.001,
            max_hz: 24_000.0,
            ..base
        },
        BandConfig {
            sample_rate: 1,
            min_hz: 0.0001,
            max_hz: 0.5,
            ..base
        },
    ];
    for c in good {
        assert!(c.validate().is_ok(), "should accept {:?}", c);
    }
}

#[test]
fn none_until_fft_size_samples() {
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&vec![0.0f32; n - 1], 1);
    assert!(a.bands().is_none());
    a.push(&[0.0f32; 1], 1);
    assert!(a.bands().is_some());
}

#[test]
fn empty_push_and_zero_channels_are_noop() {
    let cfg = BandConfig::default();
    let mut a = Analyzer::new(cfg).unwrap();
    assert!(a.bands().is_none());
    a.push(&[], 2);
    a.push(&[0.5, -0.5], 0);
    a.push(&[0.5], 3); // no complete frame
    assert!(a.bands().is_none());
    a.push(&vec![0.0f32; cfg.fft_size], 1);
    assert!(a.bands().is_some());
}

#[test]
fn silence_is_zero_after_settling() {
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&vec![0.0f32; n], 1);
    settle(&mut a, 8);
    let bands = a.bands().unwrap();
    for (i, &v) in bands.iter().enumerate() {
        assert!(v.abs() < 1e-9, "band {} = {}", i, v);
    }
}

#[test]
fn silence_falls_back_to_zero_after_tone() {
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&render_sine(cfg.sample_rate, 1000.0, 1.0, n), 1);
    settle(&mut a, 16);
    assert!(a.bands().unwrap()[expected_band(&cfg, 1000.0)] > 0.9);
    a.push(&vec![0.0f32; n], 1);
    settle(&mut a, 100);
    let bands = a.bands().unwrap();
    for (i, &v) in bands.iter().enumerate() {
        assert!(v.abs() < 1e-3, "band {} = {} after silence", i, v);
    }
}

#[test]
fn one_khz_sine_dominates_its_band() {
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&render_sine(cfg.sample_rate, 1000.0, 1.0, n), 1);
    settle(&mut a, 16);
    let bands = a.bands().unwrap();
    let peak = band_peak(bands);
    assert_eq!(peak, expected_band(&cfg, 1000.0));
    assert!(bands[peak] >= 0.9, "peak band {} = {}", peak, bands[peak]);
    for (b, &v) in bands.iter().enumerate() {
        if b.abs_diff(peak) >= 2 {
            assert!(v <= 0.3, "band {} ({} away) = {}", b, b.abs_diff(peak), v);
        }
    }
}

#[test]
fn hundred_hz_peaks_lower_than_five_khz() {
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let mut lo = Analyzer::new(cfg).unwrap();
    lo.push(&render_sine(cfg.sample_rate, 100.0, 1.0, n), 1);
    settle(&mut lo, 16);
    let mut hi = Analyzer::new(cfg).unwrap();
    hi.push(&render_sine(cfg.sample_rate, 5000.0, 1.0, n), 1);
    settle(&mut hi, 16);
    let pl = band_peak(lo.bands().unwrap());
    let ph = band_peak(hi.bands().unwrap());
    assert!(pl < ph, "100 Hz peak band {} !< 5 kHz peak band {}", pl, ph);
}

#[test]
fn stereo_silent_channel_equals_mono_half_amplitude() {
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    // Mono at half amplitude.
    let mono = render_sine(cfg.sample_rate, 1000.0, 0.5, n);
    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&mono, 1);
    settle(&mut a, 16);
    // Stereo: full-scale left, silent right -> downmix averages to half.
    let sine = render_sine(cfg.sample_rate, 1000.0, 1.0, n);
    let mut stereo = Vec::with_capacity(2 * n);
    for &s in &sine {
        stereo.push(s);
        stereo.push(0.0);
    }
    let mut b = Analyzer::new(cfg).unwrap();
    b.push(&stereo, 2);
    settle(&mut b, 16);

    let x = a.bands().unwrap();
    let y = b.bands().unwrap();
    for (i, (&u, &v)) in x.iter().zip(y).enumerate() {
        assert!((u - v).abs() < 1e-6, "band {} differs: {} vs {}", i, u, v);
    }
    // Half amplitude is ~ -6 dB -> ~0.914 (pins the normalization).
    let peak = band_peak(x);
    assert!(
        (x[peak] - 0.914).abs() < 0.03,
        "peak band = {} (want ~0.914)",
        x[peak]
    );
}

#[test]
fn band_uses_max_not_sum() {
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    // Two half-amplitude tones in the same band (band 8 ~ 799..1162 Hz).
    let s1 = render_sine(cfg.sample_rate, 900.0, 0.5, n);
    let s2 = render_sine(cfg.sample_rate, 1100.0, 0.5, n);
    let mut mix = vec![0.0f32; n];
    for i in 0..n {
        mix[i] = s1[i] + s2[i];
    }
    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&mix, 1);
    settle(&mut a, 16);
    let bands = a.bands().unwrap();
    let peak = band_peak(bands);
    assert_eq!(peak, expected_band(&cfg, 1000.0));
    // Max-of-bins keeps the reading at ~ -6 dB; a sum would read ~0 dB -> 1.0.
    assert!(
        bands[peak] < 0.97,
        "sum-style band reads {}, want < 0.97",
        bands[peak]
    );
    assert!(bands[peak] > 0.85, "peak too low: {}", bands[peak]);
}

#[test]
fn nan_inf_input_never_produces_nan() {
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let mut nasty = render_sine(cfg.sample_rate, 440.0, 1.0, n);
    for i in (0..n).step_by(3) {
        nasty[i] = f32::NAN;
    }
    for i in (1..n).step_by(5) {
        nasty[i] = f32::INFINITY;
    }
    for i in (2..n).step_by(7) {
        nasty[i] = f32::NEG_INFINITY;
    }
    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&nasty, 1);
    settle(&mut a, 16);
    let bands = a.bands().unwrap();
    for (i, &v) in bands.iter().enumerate() {
        assert!(v.is_finite(), "band {} not finite: {}", i, v);
        assert!((0.0..=1.0).contains(&v), "band {} out of range: {}", i, v);
    }

    let mut a2 = Analyzer::new(cfg).unwrap();
    a2.push(&vec![f32::NAN; n], 1);
    settle(&mut a2, 8);
    for (i, &v) in a2.bands().unwrap().iter().enumerate() {
        assert_eq!(v, 0.0, "all-NaN band {} = {}", i, v);
    }
}

#[test]
fn log_spaced_band_assignment() {
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let cases = [
        (80.0f32, 1usize),
        (100.0, 2),
        (300.0, 5),
        (1000.0, 8),
        (5000.0, 12),
        (14000.0, 15),
    ];
    for (hz, want) in cases {
        let mut a = Analyzer::new(cfg).unwrap();
        a.push(&render_sine(cfg.sample_rate, hz, 1.0, n), 1);
        settle(&mut a, 16);
        let got = band_peak(a.bands().unwrap());
        assert_eq!(
            got, want,
            "{} Hz landed in band {} (want {})",
            hz, got, want
        );
    }
}

#[test]
fn broadband_noise_clamps_to_unit_range() {
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let mut sig = vec![0.0f32; n];
    let mut seed = 0xabcdef01u32;
    for s in sig.iter_mut() {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *s = ((seed >> 8) as f32 / 16_777_216.0) * 2.0 - 1.0;
    }
    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&sig, 1);
    settle(&mut a, 16);
    let bands = a.bands().unwrap();
    for (i, &v) in bands.iter().enumerate() {
        assert!((0.0..=1.0).contains(&v), "band {} = {}", i, v);
    }
}

// --- Tests added to close audit gaps (four controller breakages) ---

#[test]
fn single_nonfinite_sample_does_not_silence_window() {
    // One NaN/+inf sample must be turned into 0.0, not poison the window. A
    // single NaN accumulated verbatim makes every FFT bin NaN (each bin is a
    // linear combination of all samples); the max-magnitude scan then sees
    // nothing above 0 and the whole window reads silent.
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let mut sig = render_sine(cfg.sample_rate, 1000.0, 1.0, n);
    sig[n / 4] = f32::NAN;
    sig[n / 2] = f32::INFINITY;
    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&sig, 1);
    settle(&mut a, 16);
    let bands = a.bands().unwrap();
    let peak = band_peak(bands);
    assert_eq!(peak, expected_band(&cfg, 1000.0));
    assert!(bands[peak] >= 0.9, "peak band {} = {}", peak, bands[peak]);
}

#[test]
fn over_full_scale_input_clamps_to_unit_range() {
    // A 4.0-amplitude sine is ~ +12 dB re full scale; the mapped level must
    // clamp to exactly 1.0 and never exceed 1.0.
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&render_sine(cfg.sample_rate, 1000.0, 4.0, n), 1);
    settle(&mut a, 16);
    let bands = a.bands().unwrap();
    for (i, &v) in bands.iter().enumerate() {
        assert!(v <= 1.0, "band {} = {} (> 1.0)", i, v);
    }
    let peak = band_peak(bands);
    assert_eq!(peak, expected_band(&cfg, 1000.0));
    assert!(
        (bands[peak] - 1.0).abs() < 1e-6,
        "peak band {} = {} (want 1.0)",
        peak,
        bands[peak]
    );
}

#[test]
fn analysis_uses_newest_fft_size_samples_in_order() {
    // Push fft_size samples of a 1 kHz sine then fft_size/2 samples of a
    // 5 kHz sine. The analyzed window must be the newest fft_size samples in
    // chronological order: reading the ring off its offset selects the same
    // samples but in a rotated order, which Hann windowing makes audible.
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let s1 = render_sine(cfg.sample_rate, 1000.0, 1.0, n);
    let s5 = render_sine(cfg.sample_rate, 5000.0, 1.0, n / 2);
    let mut seq = Vec::with_capacity(n + n / 2);
    seq.extend_from_slice(&s1);
    seq.extend_from_slice(&s5);

    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&seq, 1);
    settle(&mut a, 16);

    let newest = &seq[n / 2..];
    let mut b = Analyzer::new(cfg).unwrap();
    b.push(newest, 1);
    settle(&mut b, 16);

    let x = a.bands().unwrap();
    let y = b.bands().unwrap();
    for (i, (&u, &v)) in x.iter().zip(y).enumerate() {
        assert!((u - v).abs() < 1e-4, "band {} differs: {} vs {}", i, u, v);
    }
}

#[test]
fn impulse_at_newest_sample_is_silent() {
    // The newest sample sits at the end of the Hann window (weight ~0), so a
    // 1.0 impulse as the last pushed sample must read as silence. If the ring
    // offset is ignored, the impulse is read at window center (weight 1) and
    // lights up the bands.
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let mut a = Analyzer::new(cfg).unwrap();
    // Fill the ring so `write` lands at n/2: the impulse then goes to buf[n/2].
    a.push(&vec![0.0f32; 2 * n + n / 2], 1);
    a.push(&[1.0f32], 1);
    let bands = a.bands().unwrap();
    for (i, &v) in bands.iter().enumerate() {
        assert!(v < 1e-6, "band {} = {} (impulse leaked)", i, v);
    }
}

#[test]
fn falling_level_uses_release_not_attack() {
    // After a band settles on a sine, one silence analysis must decay by the
    // release factor (smoothing: cur += coef*(target - cur), coef = release
    // when falling, attack when rising). Using attack here drops far faster.
    let cfg = BandConfig::default();
    let n = cfg.fft_size;
    let band = expected_band(&cfg, 1000.0);
    let mut a = Analyzer::new(cfg).unwrap();
    a.push(&render_sine(cfg.sample_rate, 1000.0, 1.0, n), 1);
    settle(&mut a, 64);

    let before = a.bands().unwrap()[band];
    let t_db = before * 70.0 - 70.0; // inverse of the 0..1 dB mapping
    let floor = cfg.floor_db;
    let expected_release = t_db + cfg.release * (floor - t_db);
    let expected_attack = t_db + cfg.attack * (floor - t_db);

    a.push(&vec![0.0f32; n], 1);
    let after = a.bands().unwrap()[band];
    let after_db = after * 70.0 - 70.0;

    assert!(
        (after_db - expected_release).abs() < 0.05,
        "fell to {} dB (release predicts {}, attack predicts {})",
        after_db,
        expected_release,
        expected_attack
    );
    assert!(
        (after_db - expected_attack).abs() > 0.2,
        "fell by the attack factor: {} dB vs attack prediction {}",
        after_db,
        expected_attack
    );
}
