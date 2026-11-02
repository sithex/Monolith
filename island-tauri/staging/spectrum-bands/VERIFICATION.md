# VERIFICATION — spectrum-bands crate

Location: `island-tauri/staging/spectrum-bands/`.

Scope: pure Rust FFT spectrum analyzer for the Hermes Island equalizer visual
(SPEC §8 Phase 6 "Music extras"): interleaved PCM → small fixed number of
smoothed, log-spaced frequency bands. Platform-free: no IO, no network, no
threads, no external dependencies, no `unsafe`, no allocation after
construction.

## Gate commands — final state (all green)

```
$ nice -n 10 cargo generate-lockfile
exit=0

$ nice -n 10 cargo fmt --check
exit=0

$ nice -n 10 cargo clippy --all-targets --locked -- -D warnings
    Checking spectrum-bands v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/spectrum-bands)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s
exit=0

$ nice -n 10 cargo test --locked
   Compiling spectrum-bands v0.1.0 (/home/nick/Hermes/Ripple/island-tauri/staging/spectrum-bands)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.20s
     Running unittests src/lib.rs (target/debug/deps/spectrum_bands-2c9d93970cb1445d)

running 5 tests
test tests::ring_buffer_keeps_newest_n_samples ... ok
test tests::full_scale_on_bin_sine_normalizes_to_zero_db ... ok
test tests::buffer_capacities_preallocated ... ok
test tests::fft_matches_direct_dft ... ok
test tests::bands_does_not_reallocate_after_construction ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

     Running tests/spectrum.rs (target/debug/deps/spectrum-30c2b5656d979b79)

running 18 tests
test default_config_values_are_valid ... ok
test invalid_configs_are_rejected ... ok
test empty_push_and_zero_channels_are_noop ... ok
test none_until_fft_size_samples ... ok
test impulse_at_newest_sample_is_silent ... ok
test one_khz_sine_dominates_its_band ... ok
test over_full_scale_input_clamps_to_unit_range ... ok
test broadband_noise_clamps_to_unit_range ... ok
test band_uses_max_not_sum ... ok
test single_nonfinite_sample_does_not_silence_window ... ok
test analysis_uses_newest_fft_size_samples_in_order ... ok
test nan_inf_input_never_produces_nan ... ok
test hundred_hz_peaks_lower_than_five_khz ... ok
test silence_is_zero_after_settling ... ok
test stereo_silent_channel_equals_mono_half_amplitude ... ok
test falling_level_uses_release_not_attack ... ok
test log_spaced_band_assignment ... ok
test silence_falls_back_to_zero_after_tone ... ok

test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s

   Doc-tests spectrum_bands

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit=0
```

Total: 23 tests (5 unit + 18 integration), plus clippy `-D warnings` clean.

## Mutation checks (all actually run on the current code)

Method: snapshot pristine tree to scratch (`--exclude=target`), apply one
breakage with a fresh-mtime edit, run `nice -n 10 cargo test --locked
--no-fail-fast`, record failing test names, restore pristine, then
`diff -r --exclude=target <pristine> .` verified empty for every mutation
(the final full gate re-run on the restored tree is green, above).

Methodological note: cargo's staleness detection is mtime-based; restoring a
snapshot preserves old mtimes, which can leave a *stale artifact* from a
previous mutation in play. Every restore in this table was followed by
`touch src/lib.rs tests/spectrum.rs` (and a `cargo build` sanity check
showing `Compiling`) so the observed failures are guaranteed to come from the
mutated source — verified explicitly for M3, whose first run was stale and
had to be re-run. The table below lists the four new controller breakages
(F1–F4) added during audit follow-up, plus the six original mutations
(O1–O6) re-run unchanged under the same protocol; `diff -r --exclude=target
<pristine> .` was empty after every restore.

| # | Breakage applied | Failing tests observed |
|---|---|---|
| F1 | Non-finite sample added verbatim (`sum += s`, no `is_finite` guard) | `single_nonfinite_sample_does_not_silence_window` |
| F2 | Per-band level left unclamped (`out[band] = scaled`) | `over_full_scale_input_clamps_to_unit_range` |
| F3 | Ring order ignored (`buf[i]` instead of `buf[(start+i)%n]`) | `analysis_uses_newest_fft_size_samples_in_order`, `impulse_at_newest_sample_is_silent` |
| F4 | `attack` used for falling values instead of `release` | `falling_level_uses_release_not_attack` |
| O1 | Hann window dropped (`* self.hann[i]` removed in `bands`) | `full_scale_on_bin_sine_normalizes_to_zero_db`, `fft_matches_direct_dft`, `one_khz_sine_dominates_its_band`, `band_uses_max_not_sum`, `stereo_silent_channel_equals_mono_half_amplitude`, `impulse_at_newest_sample_is_silent`, `falling_level_uses_release_not_attack` |
| O2 | Band edges linear instead of log (`min + step*i` in `build_band_map`) | `log_spaced_band_assignment`, `one_khz_sine_dominates_its_band`, `band_uses_max_not_sum`, `silence_falls_back_to_zero_after_tone`, `over_full_scale_input_clamps_to_unit_range`, `single_nonfinite_sample_does_not_silence_window`, `falling_level_uses_release_not_attack` |
| O3 | Attack/release swapped in smoothing (`target > cur` picks release) | `stereo_silent_channel_equals_mono_half_amplitude`, `falling_level_uses_release_not_attack` |
| O4 | Sum instead of max of bins per band | `band_uses_max_not_sum`, `log_spaced_band_assignment`, `stereo_silent_channel_equals_mono_half_amplitude`, `falling_level_uses_release_not_attack` |
| O5 | Downmix takes first channel only (no averaging) | `stereo_silent_channel_equals_mono_half_amplitude`, `single_nonfinite_sample_does_not_silence_window` |
| O6 | Magnitude normalization `N/4` → `N/2` (full-scale sine reads +6 dB) | `band_uses_max_not_sum`, `stereo_silent_channel_equals_mono_half_amplitude` |

## Decisions

- **Periodic Hann window** (`w[n] = 0.5(1 − cos(2πn/N))`); its sum is exactly
  `N/2`, so a full-scale on-bin sine has DFT peak magnitude `N/4`. Magnitudes
  are normalized by `N/4` → full-scale sine reads ~0 dB. Off-bin sines read
  slightly below 0 dB (Hann scalloping ≤ ~1.4 dB), which is fine for a
  visualizer; the −6 dB half-amplitude reading is pinned by
  `stereo_silent_channel_equals_mono_half_amplitude`.
- **Band mapping**: edges are log-spaced `edge_i = min_hz·(max_hz/min_hz)^(i/B)`
  with the top edge forced exactly to `max_hz`; a bin belongs to the band whose
  `[edge_i, edge_{i+1})` interval contains its center frequency. Bands that
  would own no bin (narrow low bands) borrow the bin nearest their geometric
  center; a borrowed bin may be shared with a neighbor. Log spacing is pinned
  by `log_spaced_band_assignment` (hard-coded band indices for 80 Hz–14 kHz).
- **Per-band level** = max (not sum) of its bins' magnitudes, converted to dB,
  floored at `floor_db`, then mapped linearly `floor_db..0` dB → `0..1` and
  clamped. Max-not-sum is pinned by `band_uses_max_not_sum` (two −6 dB tones
  in one band read ~0.89, not ~1.0). The clamp is pinned by
  `over_full_scale_input_clamps_to_unit_range` (a +12 dB input reads every
  band ≤ 1.0 with the peak exactly 1.0).
- **Smoothing** is exponential in the dB domain: `cur += coef·(target − cur)`
  with `coef = attack` when rising, `release` when falling; state persists
  between calls and starts at `floor_db`, so silence reads 0 immediately and a
  stopped tone decays to 0 with `release`. Pinned by `silence_is_zero_after_settling`,
  `silence_falls_back_to_zero_after_tone`, and `one_khz_sine_dominates_its_band`;
  the falling path is additionally pinned quantitatively by
  `falling_level_uses_release_not_attack` (one silence analysis after a settled
  tone must drop by the release factor, not the attack factor).
- **Input handling**: `push` averages each complete frame of `channels`
  samples to mono (trailing partial frame ignored); non-finite samples
  contribute 0.0; empty slice or `channels == 0` is a no-op; `bands` returns
  None until `fft_size` samples have been buffered, then always analyzes the
  newest `fft_size` samples from the ring buffer. A single non-finite sample
  must never silence a whole window: `single_nonfinite_sample_does_not_silence_window`
  pins that one NaN and one +inf inside the analyzed 1 kHz window still read
  ≥ 0.9 in the 1 kHz band (a NaN accumulated verbatim poisons every FFT bin —
  each bin is a linear combination of all samples — so the earlier finiteness
  test could pass while a lone NaN silenced the window).
- **Window selection (ring order)**: `bands` reads the newest `fft_size`
  samples starting at the ring's `write` offset. Reading the raw ring ignoring
  its offset selects the same samples in a rotated order; because the Hann
  window is applied in window coordinates, that rotation is *relative to the
  window* and changes the windowed signal. Pinned by
  `analysis_uses_newest_fft_size_samples_in_order` (a push of 1 kHz then
  5 kHz matches a fresh analyzer fed exactly the newest samples in order) and
  `impulse_at_newest_sample_is_silent` (an impulse placed at the newest sample,
  where the Hann weight is ~0, reads silent).
- **No allocation after construction**: `new` preallocates the ring, Hann
  window, FFT real/imag, magnitude, band-index, smoothing-state, and output
  buffers; `bands` only reads/writes them. Checked two ways:
  `buffer_capacities_preallocated` (capacities ≥ working sizes) and
  `bands_does_not_reallocate_after_construction` (base pointers of all seven
  buffers, plus the returned slice, unchanged across 200 `bands` calls).
- **`push` never panics**: frame loop bounded by `len - i >= channels`;
  channels up to 65535 handled without overflow; f32 non-finite treated as 0.

## UNVERIFIED

- **PipeWire integration, 30 Hz emission, visibility-gated capture, pause
  handling** — Phase 6 wiring lives in `src-tauri`; this crate is the pure
  DSP only.
- **Perceptual tuning of attack/release (0.6/0.15) and floor (−70 dB)**
  against real music — chosen defaults, pinned by synthetic-signal tests only.
- **Multi-channel beyond stereo** — `channels > 2` is averaged the same way
  but not exercised with real multi-channel sources.
- **Extreme band-borrow configs** — e.g. `bands = 64` with `min_hz` very
  close to `max_hz` makes several bands borrow bins; logic is covered by the
  validator and mapping invariants but not tuned perceptually.
