# mirrorball

A [Polyend Endless](https://polyend.com/endless/) patch for the most Neil Young guitar tone possible, as on *Mirror Ball*
(1995, with Pearl Jam): Old Black's bright Firebird bridge pickup into a tweed Deluxe on 10. It's huge, woolly and
ragged, it sags and blooms into feedback, and the Bigsby shakes every chord. It's written in `no_std` Rust against the
patch ABI of Polyend's [FxPatchSDK](https://github.com/polyend/FxPatchSDK) (ABI 0x000B), so no ARM GCC is needed.

Two voices run all the time and the footswitch crossfades between them:

- **Peace** (regular): the tweed Deluxe at a low volume. It's clean when you pick lightly and breaks up when you dig
  in, like the quieter tracks.
- **Downtown** (distorted): the same amp turned up. At the default Drive it's the *Downtown* sound: a bright, mid-pushed tweed
  crunch where open chords ring clear and bark when you dig in. Two preamp stages with loose lows feed 6V6s squashing on
  a sagging tube rectifier: a hard hit squashes the attack and the chord swells back as the supply recovers. Turn Drive
  all the way up for the saturated *I'm the Ocean* / *Act of Love* wall.

Both go through an open-back 1x12 (Jensen-style) cab sim and a stereo live room (12 ms pre-delay). The room is shared,
so its tail carries across a switch. There's no noise gate: this tone is supposed to be ragged. Input comes from the
left jack.

**Feedback**: in Downtown, a note or chord left ringing (nothing picked for about a third of a second) blooms into
feedback over about a second. Like the real thing, it locks onto a harmonic of what you're holding: the note is held up
as the string decays, and the partials in a slowly drifting band (about 830 Hz to 1.5 kHz) are pushed up, so the
feedback wanders between harmonics. Picking again or muting the strings stops it.

**Bigsby**: hold the footswitch to put a hand on the bar. Everything wavers about ±20 cents at around 5 Hz, unevenly,
like Neil shaking the bar through a chord. It's on the guitar, so it works in both voices.

| Control | Function |
|---|---|
| Left knob | **Drive**: preamp gain, 0 to 38 dB. About 3% distortion at 0 (edge of breakup), 22% at the default 0.4 (Downtown), 60% at max (Ocean) |
| Mid knob | **Room**: room size and level. CCW is dry |
| Right knob | **Level**: output, unity at 3 o'clock |
| Footswitch press | **Peace ↔ Downtown**. Peace sits about 5 dB under Downtown |
| Footswitch hold | **Bigsby** wobble on/off (both voices) |

| LED | Bigsby off | Bigsby on |
|---|---|---|
| Peace | light green | light yellow |
| Downtown | dark cobalt | beige |

Red means the firmware didn't provide the working buffer.

## Build

```sh
rustup target add thumbv7em-none-eabihf --toolchain nightly
rustup component add llvm-tools --toolchain nightly
cargo build --release        # ELF for the pedal (default target is thumbv7em-none-eabihf)
./build-endl.sh              # build + make target/endless/mirrorball.endl, check f64 use and size
./build-endl.sh --deploy     # ...and copy to /Volumes/ENDLESS
```

## Audition on your computer

```sh
cargo sim                    # -> target/render/mirrorball_demo.wav
```

This renders an original, ragged D–C–G–D jam at 104 bpm, with loose timing, on a modelled electric guitar
(`src/guitar.rs`):

1. Peace: a strum a beat.
2. Downtown: bouncy, slightly swung down-up eighths at the default crunch.
3. A few-note lead ending on a held D that blooms into feedback.
4. The Bigsby: chords shaken, then a held G and D.
5. Drive turned all the way up: the Ocean wall, all-downstroke eighths hit hard.
6. A last D left to howl, then muted into the room.

Then it prints the level and stereo width of each section, how the held note's feedback bloomed, and a Drive sweep
(level and distortion of a held dyad at six knob positions).
