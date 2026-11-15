//! mirrorball: a Polyend Endless patch for the most Neil Young guitar tone
//! possible, as on Mirror Ball (1995, with Pearl Jam): Old Black's bright
//! Firebird bridge pickup into a tweed Deluxe on 10, huge, woolly and ragged,
//! sagging and blooming into feedback, the Bigsby shaking every chord.
//!
//! Two voices run all the time and the footswitch crossfades between them:
//!   Peace  the tweed Deluxe at a low volume: clean when picked lightly,
//!          breaking up when dug into (the quieter tracks)
//!   Downtown  the same amp turned up: two preamp stages, a bright, mid-pushed
//!          tweed crunch (the "Downtown" sound at the default Drive: chords
//!          ring clear and bark when dug into; Drive up is the saturated
//!          "I'm the Ocean" / "Act of Love" wall), 6V6s squashing on a sagging
//!          tube rectifier, and held notes blooming into feedback
//! then an open-back 1x12 (Jensen-style) cab sim and a stereo room shared by
//! both. Holding the footswitch puts a hand on the Bigsby: everything wobbles
//! in pitch, unevenly, like Neil shaking the bar through a chord.
//!
//! Controls
//!   Left knob   Drive   (preamp gain, 0 to 38 dB: edge of breakup -> Downtown crunch -> Ocean)
//!   Mid knob    Room    (room size and level; CCW = dry)
//!   Right knob  Level   (output; unity at 3 o'clock)
//!   Footswitch press  Peace <-> Downtown (Peace sits ~5 dB under Downtown)
//!   Footswitch hold   Bigsby wobble on/off (both voices)
//!   LED  Peace: light green | Downtown: dark cobalt | with the Bigsby: light yellow | beige
//!
//! Host builds (`cargo sim`) render a demo jam through the same code; see `host`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

/// Reusable blocks (effects, DSP, the Endless ABI); see its docs.
mod pedalkit;

/// The pedal itself.
mod mirrorball;

endless_patch!(kit = crate::pedalkit, pedal = crate::mirrorball::Patch);

#[cfg(not(target_os = "none"))]
mod host;

#[cfg(not(target_os = "none"))]
fn main() {
    host::main();
}
