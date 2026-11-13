# ai.md — Project Requirements Log

This file records every requirement given for the **mirrorball** project, so the whole project can be recreated from scratch by handing this file to an AI assistant as the prompt.

It is kept up to date as new requirements are given. Requirements are listed in the order they were given; later entries override earlier ones where they conflict (and the earlier entry is marked as superseded).

---

## Meta / Process Requirements

1. Maintain this `ai.md` file at the repo root. Its purpose is to keep a list of all requirements so the entire project can be recreated from this file alone. Update it as work progresses, whenever a new requirement is given. Keep lines unwrapped (wrap only around 200 characters at a natural break).

## Project Requirements

2. **In the existing `mirrorball` directory, build a Polyend Endless patch for the most Neil Young guitar sound on the *Mirror Ball* album (1995, with Pearl Jam)**, the same way as the sibling projects (`../gojira`, `../jesusMaryChain`): one regular and one distorted voice, a footswitch-hold effect, a host demo, built with `cargo build --release`. Pushed to `master` with the commit message `ai`.

### Implementation decisions (needed to recreate requirement 2)

- **Target sound:** Old Black (Les Paul with a Firebird mini-humbucker in the bridge and a Bigsby) into a tweed Fender Deluxe on 10: huge, woolly, ragged, sagging, blooming into feedback, the Bigsby shaken through chords.
- **Toolchain and packaging (copied unchanged from `../gojira`).** `no_std` Rust, edition 2024, nightly, `thumbv7em-none-eabihf`. Copied: `src/abi.rs`, `src/dsp.rs`, `src/guitar.rs` (host-only guitar model), `patch.ld`, `build.rs`, `.cargo/config.toml` (`-fp64`, `cargo sim` alias), `rust-toolchain.toml`, `.gitignore`. `build-endl.sh` makes `target/endless/mirrorball.endl`, checks for f64 instructions and the 512 KB limit; `--deploy` copies to `/Volumes/ENDLESS`. The reverb (`Comb`, `Allpass` with runtime lengths that start at 0, `Reverb`), the generic `Global<T>`, the entry points and the header come from gojira's `main.rs`.
- **Buffers:** the delay lines (`wob_line: Delay<128>`, `rev: Reverb`) live in a separate all-zero `Bufs` static in `.bss`, zeroed in place in `patch_init` with `core::ptr::write_bytes` before `init` (as in the sibling projects). The working buffer is unused.
- **Input:** left jack, HP 70 Hz. Envelope (1 ms attack, 50 ms release) and a slow envelope (30 ms one-pole of it). Pick onset: envelope > 1.8× the slow envelope and > 0.003. No noise gate.
- **Bigsby (footswitch hold, both voices, before both amps):** the signal always goes through a 128-sample delay line read at `2 + depth·0.5·(1 − cos 2πφ)` samples (2 samples at rest, so it never combs when switched). Depth 56 samples × hand amount (one-pole, τ 0.15 s) × `(0.7 + 0.3·sin(2π·0.37 Hz))`; rate `4.8 Hz × (1 + 0.25·sin(2π·0.23 Hz))`. Measured about −18 … +21 cents on a test tone.
- **Peace (regular voice):** HP 70 Hz → `+10 dB` into `tanh(u + 0.25) − tanh(0.25)` (2× oversampled, midpoint average) → peak 800 Hz Q0.7 +2 dB → peak 3 kHz Q0.8 +2.5 dB. Gain −5 dB + 9 dB makeup (*makeup superseded by 3: 7.5 dB*) (lands ~5 dB RMS under Ocean).
- **Ocean (distorted voice; renamed Downtown and re-voiced, see 3):**
  1. HP 60 Hz. Stage 1: gain 14–38 dB (Drive knob), `tanh(u + 0.25) − tanh(0.25)`, 2× oversampled. HP 40 Hz, LP 5.5 kHz.
  2. Stage 2: +8 dB into `tanh(u − 0.15) − tanh(−0.15)`, 2× oversampled.
  3. Tweed tone: peak 180 Hz Q0.8 +2.5 dB, peak 900 Hz Q0.7 +3 dB, peak 3 kHz Q0.8 +2 dB, HP 30 Hz.
  4. Power amp with rectifier sag: sag envelope of |y| (10 ms attack, 150 ms release); `p = 2y / (1 + 1.5·sag)`; output `(tanh(p + 0.1) − tanh(0.1)) / 2 × 1.5`.
- **Feedback (Ocean only), a pitch-locked sustainer** (*dry level superseded by 3*): a first version with a free-running delay loop (band-pass into a 307-sample delay back into the amp) bloomed at the loop's own pitch (1260 Hz on a held D5, a semitone off its harmonic), so it was replaced. Now: the bloom latches on once nothing has been picked for 0.35 s while the envelope is > 0.002, recording the envelope as the reference; it unlatches on the next onset, when the envelope falls below 0.0002 (muted), or in Peace. A ramp follows it (bloom τ 0.6 s, die τ 0.08 s). The amp input is `x + (hold·(0.3·x + 1.5·BP(x)) − x)·ramp`, with `hold = min(ref / env, +30 dB)` and BP a band-pass Q4 centred `1100·2^(0.4·sin(2π·0.07 Hz))` Hz (retuned every 32 samples). On the held D5 the output shifts between 587, 1174 and 1761 Hz (its harmonics) and stays near its starting level.
- **Mix and output:** 12 ms crossfade between voices (mono) → open-back 1x12 cab: HP 70 Q0.7, peak 200 Hz Q1 +2 dB, peak 1.2 kHz Q1 +2 dB, peak 2.8 kHz Q1.5 +3 dB, LP 5 kHz Q0.7, LP 7.5 kHz Q0.6. Then the shared stereo room: Freeverb as in gojira but 12 ms pre-delay (576 samples), damping 0.45, feedback `0.70 + 0.2·Room`, wet `Room × (0.7 Peace … 0.5 Ocean)` following the crossfade. Output `(dry + wet·rev) × Level² (unity at 0.75) × 0.36` (*trim superseded by 3: 0.42*), then `tanh × 0.98`.
- **Controls** (*Drive default and voice name superseded by 3*)**:** Drive (default 0.75), Room (default 0.45), Level (default 0.75). Press: Peace ↔ Ocean (starts in Ocean). Hold: Bigsby on/off. LED: Peace light green / light yellow, Ocean dark cobalt / beige (Bigsby off / on). Red means the working buffer is missing.
- **Host audition** (*superseded by 3*)**:** `cargo sim` renders an original D–C–G–D jam (open D, C, G voicings) at 104 bpm, input ×0.6, ±12 ms timing slop: Peace (a strum a beat, 2 rounds) → Ocean (seven all-downstroke eighths per bar, the last held for a quarter, 2 rounds) → a seven-note lead (A bent up a tone, F#, A, B, A let down a semitone, F#, D, all with vibrato) → a held D5 (T60 30 s, 2 bars) for the feedback bloom → hold: one round of quarter-note chords, then G and D held a bar each (T60 30 s) → hold off: a last D held 2 bars → mute → 3 s tail. Output `target/render/mirrorball_demo.wav`, with per-section peak/RMS/side/correlation stats and the held note's bloom (level and sustain ramp early vs. in its last second); it fails on NaN or full-scale output.

3. **Voice the distorted sound mainly for "Downtown"** (the looser, poppier crunch from *Mirror Ball*) rather than the saturated "I'm the Ocean" / "Act of Love" wall, keeping that wall reachable. Pushed to `master` with the commit message `ai`.

### Implementation decisions (needed to recreate requirement 3)

- **Measured first:** a Drive sweep (below) showed the old voice saturated everywhere (19% distortion at Drive 0, 51% at the default 0.75), too hot for Downtown.
- **Downtown (the distorted voice, renamed from Ocean):** stage 1 gain now 0–38 dB (was 14–38), default Drive 0.4 (`DRIVE_DEFAULT`). Measured on the held dyad: 3.3% at 0, 9.5% at 0.2, 18.6% at 0.35, ~22% at 0.4, 31% at 0.5, 50% at 0.75, 61% at 1. Tone: interstage LP 7 kHz (was 5.5 kHz), low peak 180 Hz +1 dB (was +2.5), treble peak 3 kHz +3.5 dB (was +2) so open chords stay clear; mid peak 900 Hz +3 dB, stage 2, sag and power amp unchanged.
- **Feedback:** `FB_DRY` 0.5 (was 0.3), because with less amp compression the held note sagged 5.5 dB; now −20.3 → −22.5 dB over the held note. The +30 dB makeup cap stays (more would bring up pickup hiss on a real guitar).
- **Levels:** output trim 0.42 (Downtown chords ≈ −14.4 dB RMS), Peace makeup 7.5 dB (Peace ≈ 4.9 dB under Downtown).
- **Controls:** Drive default 0.4. Press: Peace ↔ Downtown (starts in Downtown). LED: Downtown dark cobalt / beige; Peace unchanged.
- **Host audition:** as in 2, except: the Ocean chord section becomes Downtown — down-up eighths, downstrokes full chords (0.95 on 1, else 0.8), upstrokes the top 3 strings at 0.6, 0.010 / 0.006 s strums, upstrokes swung late by 0.12 of an eighth, 2 rounds; the lead, held note and Bigsby sections are labelled Downtown; after the Bigsby (hold off) a new section turns Drive to 1.0 (an `Ev::Drive` event) for one round of the old all-downstroke Ocean eighths, then the last D howls and is muted. After the render, a Drive sweep holds a 147 + 220 Hz sine dyad (0.08 each) through the distorted voice (room off, Bigsby off, the sustainer held off by resetting `since_onset`) at Drive 1, 0.75, 0.5, 0.35, 0.2, 0, and prints level and distortion % (everything that isn't the two tones), skipping the first 0.5 s and measuring 1 s.
