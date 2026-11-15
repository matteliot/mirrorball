//! Host-only demo: `cargo sim [OUT.wav]` plays `score` on the modelled
//! guitar through the patch (see `pedalkit::host`), reports the feedback
//! bloom on the held lead note, then sweeps Drive on a held dyad.

mod score;

use crate::entry::{ENTRY, pedal, set_param, update_buffers};
use crate::pedalkit::abi::PatchEnv;
use crate::pedalkit::host::stats::to_db;
use crate::pedalkit::host::{self, BLOCK, Options, SRU};

const PARAM_DRIVE: i32 = 0;
const PARAM_ROOM: i32 = 1;

pub fn main() {
    let out = host::out_path("target/render/mirrorball_demo.wav");
    let (events, total, marks) = score::score();
    let o = Options { first: "peace (tweed turned down)", input_trim: 0.6, working: 16 };
    let mut ramp = vec![];
    let r = host::render_with(&ENTRY, events, total, o, &mut || ramp.push(pedal().fb_ramp()));
    let lo = &r.l;
    // Feedback bloom on the held lead note: level and the sustainer's ramp early
    // on, and late, when the amp has taken the note over.
    let rms_db = |a: f64, b: f64| {
        let (a, b) = ((a * SRU as f64) as usize, (b * SRU as f64) as usize);
        let s = &lo[a..b];
        10.0 * (s.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / s.len() as f64).max(1e-18).log10()
    };
    let ramp_at = |s: f64| ramp[(s * SRU as f64) as usize / BLOCK];
    let (b0, b1) = (marks.bloom, marks.bloom + marks.bloom_len);
    let bloom = format!(
        "  feedback bloom: {:.1} dB, sustain {:.2} (0.2-0.6 s in) -> {:.1} dB, sustain {:.2} (last second)",
        rms_db(b0 + 0.2, b0 + 0.6),
        ramp_at(b0 + 0.4),
        rms_db(b1 - 1.0, b1),
        ramp_at(b1 - 0.1)
    );
    let ok = host::stats::report(&r.l, &r.r, r.sections.clone());
    println!("{bloom}");
    println!("final LED colour index: {}", unsafe { (ENTRY.get_state_idx)(&r.env) });
    host::wav::write_wav(&out, &r.l, &r.r);
    println!("wrote {out} ({:.1} s)", r.l.len() as f64 / SRU as f64);
    drive_sweep(&r.env);
    if !ok {
        eprintln!("error: NaN or full-scale output");
        std::process::exit(1);
    }
}

/// Holds a D3 + A3 dyad (two sines) through the distorted voice (no Bigsby or room) at several Drive
/// settings, and prints the output level and the distortion (everything that isn't the two input notes,
/// relative to them, as an amplitude ratio).
fn drive_sweep(env: &PatchEnv) {
    pedal().analysis(true, false);
    unsafe { set_param(env, PARAM_ROOM, 0.0) };
    let tones = [147.0, 220.0]; // D3 and A3, rounded so both fit the 1 s window exactly (no leakage)
    println!("\nDrive sweep, held D3+A3 dyad through the distorted voice:\n{:>6} {:>8} {:>12}", "Drive", "rms dB", "distortion %");
    for drive in [1.0, 0.75, 0.5, 0.35, 0.2, 0.0] {
        unsafe { set_param(env, PARAM_DRIVE, drive) };
        let mut out = vec![];
        let mut n = 0usize;
        while out.len() < SRU * 3 / 2 {
            let mut l = [0.0f32; BLOCK];
            let mut r = [0.0f32; BLOCK];
            for x in l.iter_mut() {
                let t = n as f32 / SRU as f32;
                *x = tones.iter().map(|f| 0.08 * (core::f32::consts::TAU * f * t).sin()).sum();
                n += 1;
            }
            pedal().analysis(true, false); // keep the sustainer from latching on the steady tones
            unsafe { update_buffers(env, l.as_mut_ptr(), r.as_mut_ptr(), BLOCK as i32) };
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
