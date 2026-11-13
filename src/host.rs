//! Host-only demo renderer: `cargo sim [OUT.wav]`.
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

use crate::guitar::{Note, Player};
use crate::*;

const N: usize = 128;
const SRU: usize = 48_000;
/// The modelled guitar into this patch, matched to the level the patch was voiced for.
const INPUT_TRIM: f32 = 0.6;

// Open-position chord voicings, low string to high.
const D: [f32; 4] = [146.83, 220.0, 293.66, 369.99];
const C: [f32; 5] = [130.81, 164.81, 196.0, 261.63, 329.63];
const G: [f32; 6] = [98.0, 123.47, 146.83, 196.0, 293.66, 392.0];

enum Ev {
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
struct Marks {
    bloom: f64,
    bloom_len: f64,
}

fn score() -> (Vec<(f64, Ev, &'static str)>, f64, Marks) {
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
    ev.push((0.0, Ev::Press, ""));
    for _ in 0..2 {
        for c in prog {
            for k in 0..4 {
                let vel = if k == 0 { 0.75 } else { 0.6 };
                ev.push((t + slop(), Ev::Play(chord(c, vel), 0.014, false), ""));
                t += q;
            }
        }
    }

    // Downtown: bouncy down-up eighths with a little swing (upstrokes late,
    // lighter, top strings only), the "and" of 4 left to ring into the next
    // chord.
    let swing = 0.12 * e;
    let up = |c: &[f32], vel: f32| c[c.len().saturating_sub(3)..].iter().map(|f| Note::new(*f).vel(vel)).collect::<Vec<_>>();
    ev.push((t, Ev::Press, "downtown (chords)"));
    for _ in 0..2 {
        for c in prog {
            for k in 0..8 {
                let x = if k % 2 == 0 {
                    Ev::Play(chord(c, if k == 0 { 0.95 } else { 0.8 }), 0.010, false)
                } else {
                    Ev::Play(up(c, 0.6), 0.006, true)
                };
                let at = t + if k % 2 == 1 { swing } else { 0.0 };
                ev.push((at + slop(), x, ""));
                t += e;
            }
        }
    }

    // A few-note lead, worried rather than shredded, ending on a held D that
    // the amp takes over.
    ev.push((t, Ev::Play(vec![], 0.0, true), "downtown (lead)"));
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
        ev.push((t + slop(), Ev::Play(vec![n], 0.0, false), ""));
        t += eighths * e;
    }
    let bloom = t;
    let bloom_len = 8.0 * q;
    ev.push((t, Ev::Play(vec![Note::new(587.33).vel(1.0).t60(30.0).vib()], 0.0, false), "downtown, held note (feedback)"));
    t += bloom_len;

    // The Bigsby: a round of chords shaken, then a held G and D.
    ev.push((t, Ev::Hold, "downtown + bigsby"));
    for c in prog {
        for k in 0..4 {
            ev.push((t + slop(), Ev::Play(chord(c, if k == 0 { 1.0 } else { 0.85 }), 0.010, false), ""));
            t += q;
        }
    }
    ev.push((t + slop(), Ev::Play(held(&G, 1.0), 0.012, false), ""));
    t += 4.0 * q;
    ev.push((t + slop(), Ev::Play(held(&D, 1.0), 0.012, false), ""));
    t += 4.0 * q;

    // Bigsby off, Drive all the way up: the saturated Ocean wall, all-
    // downstroke eighths hit hard, the last of each bar left to ring.
    ev.push((t, Ev::Hold, "drive up: ocean (chords)"));
    ev.push((t, Ev::Drive(1.0), ""));
    for c in prog {
        for k in 0..7 {
            let vel = if k == 0 { 1.0 } else { 0.85 };
            ev.push((t + slop(), Ev::Play(chord(c, vel), 0.010, false), ""));
            t += if k == 6 { 2.0 * e } else { e };
        }
    }

    // A last D left to howl, then muted into the room.
    ev.push((t + 0.01, Ev::Play(held(&D, 1.0), 0.014, false), "last chord, howl"));
    t += 8.0 * q;
    ev.push((t, Ev::Play(vec![Note::new(146.83).vel(0.01).t60(0.02)], 0.0, false), "muted: room tail"));
    (ev, t + 3.0, Marks { bloom, bloom_len })
}

pub fn main() {
    let out = std::env::args().nth(1).unwrap_or("target/render/mirrorball_demo.wav".into());
    let env = PatchEnv { abi_version: PATCH_ABI_VERSION as u32, user_ctx: core::ptr::null_mut() };
    let mut work = vec![0.0f32; 16];
    unsafe {
        patch_init(&env);
        set_buffer(&env, work.as_mut_ptr());
        for i in 0..PARAM_COUNT {
            set_param(&env, i, get_param_default(&env, i));
        }
    }

    let (mut events, total_s, marks) = score();
    events.sort_by(|a, b| b.0.total_cmp(&a.0));
    let total = (total_s * SRU as f64) as usize;
    let mut guitar = Player::new();

    let mut lo = vec![];
    let mut ro = vec![];
    let mut ramp = vec![];
    let mut sections: Vec<(usize, &str)> = vec![(0, "peace (tweed turned down)")];
    let mut t = 0;
    while t < total {
        while events.last().is_some_and(|(et, _, _)| (*et * SRU as f64) as usize <= t) {
            let (_, e, label) = events.pop().unwrap();
            match e {
                Ev::Press => unsafe { special_action(&env, ACTION_FOOTSWITCH_PRESS) },
                Ev::Hold => unsafe { special_action(&env, ACTION_FOOTSWITCH_HOLD) },
                Ev::Drive(v) => unsafe { set_param(&env, PARAM_DRIVE, v) },
                Ev::Play(notes, strum, ring) => guitar.play(&notes, strum, ring),
            }
            if !label.is_empty() {
                sections.push((t, label));
            }
        }
        let mut l = [0.0f32; N];
        let mut r = [0.0f32; N];
        for x in l.iter_mut() {
            *x = guitar.tick() * INPUT_TRIM;
        }
        unsafe { update_buffers(&env, l.as_mut_ptr(), r.as_mut_ptr(), N as i32) };
        lo.extend_from_slice(&l);
        ro.extend_from_slice(&r);
        ramp.push(state().fb_ramp);
        t += N;
    }

    // Feedback bloom on the held lead note: level and the sustainer's ramp early
    // on, and late, when the amp has taken the note over.
    let rms_db = |a: f64, b: f64| {
        let (a, b) = ((a * SRU as f64) as usize, (b * SRU as f64) as usize);
        let s = &lo[a..b];
        10.0 * (s.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / s.len() as f64).max(1e-18).log10()
    };
    let ramp_at = |s: f64| ramp[(s * SRU as f64) as usize / N];
    let (b0, b1) = (marks.bloom, marks.bloom + marks.bloom_len);
    let bloom = format!(
        "  feedback bloom: {:.1} dB, sustain {:.2} (0.2-0.6 s in) -> {:.1} dB, sustain {:.2} (last second)",
        rms_db(b0 + 0.2, b0 + 0.6),
        ramp_at(b0 + 0.4),
        rms_db(b1 - 1.0, b1),
        ramp_at(b1 - 0.1)
    );

    let mut bad = false;
    sections.push((lo.len(), ""));
    println!("{:<30} {:>7} {:>8} {:>8} {:>8}", "section", "peak", "rms dB", "L-R dB", "L/R corr");
    for w in sections.windows(2) {
        let (a, name) = w[0];
        let b = w[1].0;
        let (l, r) = (&lo[a..b], &ro[a..b]);
        let peak = l.iter().chain(r).fold(0.0f32, |m, v| m.max(v.abs()));
        let rms = |x: &[f32]| (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
        let side: Vec<f32> = l.iter().zip(r).map(|(a, b)| a - b).collect();
        let dot: f64 = l.iter().zip(r).map(|(a, b)| *a as f64 * *b as f64).sum();
        let corr = dot / (rms(l) * rms(r) * l.len() as f64).max(1e-12);
        let to_db = |v: f64| 20.0 * v.max(1e-9).log10();
        println!("{name:<30} {peak:>7.3} {:>8.1} {:>8.1} {corr:>8.2}", to_db(rms(l)), to_db(rms(&side)));
        if l.iter().chain(r).any(|v| !v.is_finite()) || peak >= 1.0 {
            bad = true;
        }
    }
    println!("{bloom}");
    println!("final LED colour index: {}", unsafe { get_state_idx(&env) });

    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    write_wav(&out, &lo, &ro);
    println!("wrote {out} ({:.1} s)", lo.len() as f64 / SRU as f64);
    drive_sweep(&env);
    if bad {
        eprintln!("error: NaN or full-scale output");
        std::process::exit(1);
    }
    let _ = (get_buffer_size, get_param_min, get_param_max, is_param_enabled, get_param_name, get_param_unit);
}

fn write_wav(path: &str, l: &[f32], r: &[f32]) {
    let bytes = (l.len() * 8) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + bytes).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&48_000u32.to_le_bytes());
    b.extend_from_slice(&(48_000u32 * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&bytes.to_le_bytes());
    for (a, c) in l.iter().zip(r) {
        b.extend_from_slice(&a.to_le_bytes());
        b.extend_from_slice(&c.to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}

/// How hard the distorted voice breaks up across the Drive knob: a held D3+A3 dyad (sine tones at a strummed
/// chord's level), level and distortion (everything that isn't the two tones). The feedback sustainer is held off
/// so this measures the amp alone.
fn drive_sweep(env: &PatchEnv) {
    let s = state();
    s.loud = true;
    s.bigsby = false;
    unsafe { set_param(env, PARAM_ROOM, 0.0) };
    let tones = [147.0, 220.0]; // D3 and A3, rounded so both fit the 1 s window exactly (no leakage)
    let to_db = |v: f64| 20.0 * v.max(1e-9).log10();
    println!("\nDrive sweep, held D3+A3 dyad through the distorted voice:\n{:>6} {:>8} {:>12}", "Drive", "rms dB", "distortion %");
    for drive in [1.0, 0.75, 0.5, 0.35, 0.2, 0.0] {
        unsafe { set_param(env, PARAM_DRIVE, drive) };
        let mut out = vec![];
        let mut n = 0usize;
        while out.len() < SRU * 3 / 2 {
            let mut l = [0.0f32; N];
            let mut r = [0.0f32; N];
            for x in l.iter_mut() {
                let t = n as f32 / SRU as f32;
                *x = tones.iter().map(|f| 0.08 * (core::f32::consts::TAU * f * t).sin()).sum();
                n += 1;
            }
            state().since_onset = 0; // keep the sustainer from latching on the steady tones
            unsafe { update_buffers(env, l.as_mut_ptr(), r.as_mut_ptr(), N as i32) };
            out.extend_from_slice(&l);
        }
        // Skip the first half second (knob smoothing, filters settling), then measure one second.
        let w = &out[SRU / 2..SRU / 2 + SRU];
        let total: f64 = w.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / w.len() as f64;
        let tone_power: f64 = tones
            .iter()
            .map(|f| {
                let (mut c, mut si) = (0.0f64, 0.0f64);
                for (i, v) in w.iter().enumerate() {
                    let ph = std::f64::consts::TAU * *f as f64 * (SRU / 2 + i) as f64 / SRU as f64;
                    c += *v as f64 * ph.cos();
                    si += *v as f64 * ph.sin();
                }
                let (a, b) = (2.0 * c / w.len() as f64, 2.0 * si / w.len() as f64);
                (a * a + b * b) / 2.0
            })
            .sum();
        println!("{drive:>6.2} {:>8.1} {:>12.1}", to_db(total.sqrt()), 100.0 * ((total - tone_power).max(0.0) / tone_power).sqrt());
    }
}
