//! The `cargo sim` demo: its score (the rendering itself is `pedalkit::host`).
//!
//! Plays an original, ragged D-C-G-D jam at 104 bpm on a modelled electric
//! guitar (see `guitar`: exact tuning, pick and pickup, strums, human
//! timing; the timing here is loose on purpose), drives the patch's entry
//! points the way the firmware does (128-sample blocks, footswitch events at
//! set times) and writes a stereo float WAV: Peace (the tweed turned down,
//! strummed) -> Downtown (the default crunch, bouncy down-up eighths) -> a
//! sparse lead ending on a held note that blooms into feedback -> the Bigsby
//! (chords shaken, then held) -> Drive turned all the way up (the saturated
//! Ocean wall, all-downstroke eighths) -> a last chord left to howl, then
//! muted. Prints per-section level stats, the feedback bloom and a Drive
//! sweep, and fails on NaN or clipping.

use crate::pedalkit::guitar::Note;
use crate::pedalkit::host::{Ev, Score};

// Open-position chord voicings, low string to high.
const D: [f32; 4] = [146.83, 220.0, 293.66, 369.99];
const C: [f32; 5] = [130.81, 164.81, 196.0, 261.63, 329.63];
const G: [f32; 6] = [98.0, 123.47, 146.83, 196.0, 293.66, 392.0];

enum Local {
    /// Play these notes, strummed this many seconds apart; `ring` lets what's already sounding carry on.
    Play(Vec<Note>, f32, bool),
    Press,
    Hold,
    /// Turn the Drive knob to this.
    Drive(f32),
}

fn chord(c: &[f32], vel: f32) -> Vec<Note> {
    c.iter().map(|f| Note::new(*f).vel(vel)).collect()
}

/// A chord left ringing in front of a loud amp (it keeps the strings going).
fn held(c: &[f32], vel: f32) -> Vec<Note> {
    c.iter().map(|f| Note::new(*f).vel(vel).t60(30.0)).collect()
}

/// Start of the held lead note, and how long it's held (s): for the bloom stats.
pub struct Marks {
    pub bloom: f64,
    pub bloom_len: f64,
}

fn local_score() -> (Vec<(f64, Local, &'static str)>, f64, Marks) {
    let mut ev = vec![];
    let q = 60.0 / 104.0; // quarter note
    let e = q / 2.0;
    let mut t = 0.1;
    // Loose timing: a band playing live in a room, not to a click.
    let mut seed: u32 = 0x1357_9bdf;
    let mut slop = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f64 / u32::MAX as f64 * 2.0 - 1.0) * 0.012
    };
    let prog: [&[f32]; 4] = [&D, &C, &G, &D];

    // Peace: a strum a beat, a bar per chord, twice round.
    ev.push((0.0, Local::Press, ""));
    for _ in 0..2 {
        for c in prog {
            for k in 0..4 {
                let vel = if k == 0 { 0.75 } else { 0.6 };
                ev.push((t + slop(), Local::Play(chord(c, vel), 0.014, false), ""));
                t += q;
            }
        }
    }

    // Downtown: bouncy down-up eighths with a little swing (upstrokes late,
    // lighter, top strings only), the "and" of 4 left to ring into the next
    // chord.
    let swing = 0.12 * e;
    let up = |c: &[f32], vel: f32| c[c.len().saturating_sub(3)..].iter().map(|f| Note::new(*f).vel(vel)).collect::<Vec<_>>();
    ev.push((t, Local::Press, "downtown (chords)"));
    for _ in 0..2 {
        for c in prog {
            for k in 0..8 {
                let x = if k % 2 == 0 {
                    Local::Play(chord(c, if k == 0 { 0.95 } else { 0.8 }), 0.010, false)
                } else {
                    Local::Play(up(c, 0.6), 0.006, true)
                };
                let at = t + if k % 2 == 1 { swing } else { 0.0 };
                ev.push((at + slop(), x, ""));
                t += e;
            }
        }
    }

    // A few-note lead, worried rather than shredded, ending on a held D that
    // the amp takes over.
    ev.push((t, Local::Play(vec![], 0.0, true), "downtown (lead)"));
    let lead: [(f32, f64, Option<f32>); 7] = [
        (440.0, 2.0, Some(200.0)), // A, bent up a tone
        (369.99, 2.0, None),
        (440.0, 1.0, None),
        (493.88, 1.0, None),
        (440.0, 2.0, Some(-100.0)), // let down flat, Bigsby-style
        (369.99, 3.0, None),
        (293.66, 5.0, None),
    ];
    for (f, eighths, bend) in lead {
        let mut n = Note::new(f).vel(0.95).vib();
        if let Some(c) = bend {
            n = n.bend(0.08, 0.18, c);
        }
        ev.push((t + slop(), Local::Play(vec![n], 0.0, false), ""));
        t += eighths * e;
    }
    let bloom = t;
    let bloom_len = 8.0 * q;
    ev.push((t, Local::Play(vec![Note::new(587.33).vel(1.0).t60(30.0).vib()], 0.0, false), "downtown, held note (feedback)"));
    t += bloom_len;

    // The Bigsby: a round of chords shaken, then a held G and D.
    ev.push((t, Local::Hold, "downtown + bigsby"));
    for c in prog {
        for k in 0..4 {
            ev.push((t + slop(), Local::Play(chord(c, if k == 0 { 1.0 } else { 0.85 }), 0.010, false), ""));
            t += q;
        }
    }
    ev.push((t + slop(), Local::Play(held(&G, 1.0), 0.012, false), ""));
    t += 4.0 * q;
    ev.push((t + slop(), Local::Play(held(&D, 1.0), 0.012, false), ""));
    t += 4.0 * q;

    // Bigsby off, Drive all the way up: the saturated Ocean wall, all-
    // downstroke eighths hit hard, the last of each bar left to ring.
    ev.push((t, Local::Hold, "drive up: ocean (chords)"));
    ev.push((t, Local::Drive(1.0), ""));
    for c in prog {
        for k in 0..7 {
            let vel = if k == 0 { 1.0 } else { 0.85 };
            ev.push((t + slop(), Local::Play(chord(c, vel), 0.010, false), ""));
            t += if k == 6 { 2.0 * e } else { e };
        }
    }

    // A last D left to howl, then muted into the room.
    ev.push((t + 0.01, Local::Play(held(&D, 1.0), 0.014, false), "last chord, howl"));
    t += 8.0 * q;
    ev.push((t, Local::Play(vec![Note::new(146.83).vel(0.01).t60(0.02)], 0.0, false), "muted: room tail"));
    (ev, t + 3.0, Marks { bloom, bloom_len })
}

/// The score as pedalkit events, and where the held lead note blooms.
pub fn score() -> (Score, f64, Marks) {
    let (ev, total, marks) = local_score();
    (ev.into_iter().map(|(t, e, l)| (t, e.into(), l)).collect(), total, marks)
}

impl From<Local> for Ev {
    fn from(e: Local) -> Ev {
        match e {
            Local::Play(notes, strum, ring) => Ev::Play(notes, strum, ring),
            Local::Press => Ev::Press,
            Local::Hold => Ev::Hold,
            Local::Drive(v) => Ev::Knob(0, v),
        }
    }
}
