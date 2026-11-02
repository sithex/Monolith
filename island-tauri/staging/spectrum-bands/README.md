# spectrum-bands

Pure Rust FFT spectrum analyzer for the Hermes Island equalizer visual
(SPEC §8 Phase 6 "Music extras"). Turns interleaved PCM audio samples into a
small fixed number of smoothed, log-spaced frequency bands in `[0, 1]`.

- No dependencies, no IO, no threads, no `unsafe` (`#![forbid(unsafe_code)]`).
- Own iterative radix-2 FFT, periodic Hann window, magnitude normalization so
  a full-scale sine reads ~0 dB.
- Log-spaced band edges; each band is the max of its bins' dB levels, mapped
  linearly `floor_db..0` dB → `0..1` and clamped; every band gets at least one
  bin (narrow low bands borrow the nearest bin).
- Exponential per-band smoothing: `attack` for rising values, `release` for
  falling, state kept between calls.
- No allocation after construction — all buffers preallocated in `new`.

## Usage

```rust
use spectrum_bands::{Analyzer, BandConfig};

let mut analyzer = Analyzer::new(BandConfig::default())?; // 48 kHz, 2048, 16 bands
analyzer.push(&interleaved_f32_samples, 2);               // stereo -> mono avg
if let Some(bands) = analyzer.bands() {
    // newest 2048 samples -> 16 smoothed levels in [0, 1]
}
```

`bands()` returns `None` until `fft_size` samples have been buffered. Validate
a custom config with `BandConfig::validate()` before constructing.

See `VERIFICATION.md` for gate output, mutation checks, and design decisions.
