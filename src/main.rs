//! mirrorball: a Polyend Endless patch for the most Neil Young guitar tone
//! possible, as on Mirror Ball (1995, with Pearl Jam): Old Black's bright
//! Firebird bridge pickup into a tweed Deluxe on 10, huge, woolly and ragged,
//! sagging and blooming into feedback, the Bigsby shaking every chord.
//!
//! Two voices run all the time and the footswitch crossfades between them:
//!   Peace  the tweed Deluxe at a low volume: clean when picked lightly,
//!          breaking up when dug into (the quieter tracks)
//!   Ocean  the same amp flat out: two preamp stages, a woolly, mid-pushed
//!          tweed tone, 6V6s squashing on a sagging tube rectifier, and
//!          held notes blooming into feedback ("I'm the Ocean", "Act of Love")
//! then an open-back 1x12 (Jensen-style) cab sim and a stereo room shared by
//! both. Holding the footswitch puts a hand on the Bigsby: everything wobbles
//! in pitch, unevenly, like Neil shaking the bar through a chord.
//!
//! Controls
//!   Left knob   Drive   (Ocean's preamp gain, ~14 to 38 dB)
//!   Mid knob    Room    (room size and level; CCW = dry)
//!   Right knob  Level   (output; unity at 3 o'clock)
//!   Footswitch press  Peace <-> Ocean (Peace sits ~5 dB under Ocean)
//!   Footswitch hold   Bigsby wobble on/off (both voices)
//!   LED  Peace: light green | Ocean: dark cobalt | with the Bigsby: light yellow | beige
//!
//! Host builds (`cargo sim`) render a demo jam through the same code; see `host`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod abi;
#[allow(dead_code)]
mod dsp;

use core::cell::UnsafeCell;
use core::f32::consts::TAU;

use abi::*;
use dsp::*;

// ---------------------------------------------------------------- voicing

/// Ocean preamp stage 1 gain (dB) at knob 0, and the extra at knob 1.
const DRIVE_MIN_DB: f32 = 14.0;
const DRIVE_RANGE_DB: f32 = 24.0;
/// Ocean preamp stage 2 gain, dB.
const DRIVE2_DB: f32 = 8.0;
/// Stage biases: asymmetric clipping (even harmonics, like a tube stage).
const BIAS1: f32 = 0.25;
const BIAS2: f32 = -0.15;
/// Power amp: drive into the 6V6s and their bias.
const POWER_DRIVE: f32 = 2.0;
const POWER_BIAS: f32 = 0.1;
/// Rectifier sag: how far a loud signal pulls the power amp's gain down, and
/// the supply's attack / recovery (s). The recovery is what makes a held
/// chord swell back up after the pick hits.
const SAG: f32 = 1.5;
const SAG_ATT_S: f32 = 0.01;
const SAG_REL_S: f32 = 0.15;

/// Peace: the same amp at a low volume. Preamp gain (dB), and the voice's
/// level under Ocean plus its makeup gain so it lands there.
const PEACE_DRIVE_DB: f32 = 10.0;
const PEACE_LEVEL_DB: f32 = -5.0;
const PEACE_MAKEUP_DB: f32 = 9.0;

/// Feedback (Ocean only). Real feedback locks onto a harmonic of the string,
/// because the string is the resonator; a free-running loop in the pedal
/// would whistle at its own pitch instead. So this is a sustainer: a held
/// note blooms once nothing has been picked for FB_WAIT seconds while the
/// string is still ringing above FB_HOLD. The note is then held at the level
/// it had (up to FB_MAX_DB of makeup as the string decays), its body pulled
/// down to FB_DRY and the partials inside a slowly drifting band (centre,
/// Q, octaves of drift, drift rate) pushed up by FB_HARM, so the amp locks
/// onto a harmonic of whatever is held and wanders between harmonics. It
/// holds until the next pick or until the strings are muted below FB_MUTE.
/// Bloom / die times (s).
const FB_WAIT: f32 = 0.35;
const FB_HOLD: f32 = 0.002;
const FB_MUTE: f32 = 0.0002;
const FB_MAX_DB: f32 = 30.0;
const FB_DRY: f32 = 0.3;
const FB_HARM: f32 = 1.5;
const FB_FREQ: f32 = 1100.0;
const FB_Q: f32 = 4.0;
const FB_WANDER: f32 = 0.4;
const FB_RATE: f32 = 0.07;
const FB_BLOOM_S: f32 = 0.6;
const FB_DIE_S: f32 = 0.08;
/// Pick onset detector: fast envelope this many times the slow one, above
/// this level.
const ONSET: f32 = 1.8;
const ONSET_MIN: f32 = 0.003;

/// Bigsby: the base delay and the swing (samples; ~ +-20 cents at the
/// centre rate), the shake rate (Hz), how much it drifts (fraction, at
/// WOB_DRIFT_HZ), how much the depth varies (at WOB_SWAY_HZ) and how fast
/// the hand gets on / off the bar (s).
const WOB_BASE: f32 = 2.0;
const WOB_DEPTH: f32 = 56.0;
const WOB_RATE: f32 = 4.8;
const WOB_DRIFT: f32 = 0.25;
const WOB_DRIFT_HZ: f32 = 0.23;
const WOB_SWAY: f32 = 0.3;
const WOB_SWAY_HZ: f32 = 0.37;
const WOB_HAND_S: f32 = 0.15;

/// Room: wet level at full Room for each voice, feedback range, damping and
/// pre-delay (samples, 12 ms): a live room, not a hall.
const PEACE_WET: f32 = 0.7;
const OCEAN_WET: f32 = 0.5;
const ROOM_MIN: f32 = 0.70;
const ROOM_RANGE: f32 = 0.2;
const DAMP: f32 = 0.45;
const PREDELAY: f32 = 576.0;

/// Output trim: unity Level puts Ocean's chords around -14 dB RMS.
const OUT_TRIM: f32 = 0.36;

// ---------------------------------------------------------------- reverb

/// Freeverb-style lowpass-feedback comb with a runtime length.
struct Comb<const N: usize> {
    buf: [f32; N],
    len: usize,
    i: usize,
    filt: f32,
}

impl<const N: usize> Comb<N> {
    const NEW: Self = Self { buf: [0.0; N], len: 0, i: 0, filt: 0.0 };

    #[inline]
    fn process(&mut self, x: f32, fb: f32, damp: f32) -> f32 {
        let o = self.buf[self.i];
        self.filt = o + (self.filt - o) * damp;
        self.buf[self.i] = x + self.filt * fb;
        self.i += 1;
        if self.i >= self.len {
            self.i = 0;
        }
        o
    }
}

/// Schroeder allpass (feedback 0.5) with a runtime length.
struct Allpass<const N: usize> {
    buf: [f32; N],
    len: usize,
    i: usize,
}

impl<const N: usize> Allpass<N> {
    const NEW: Self = Self { buf: [0.0; N], len: 0, i: 0 };

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let o = self.buf[self.i];
        self.buf[self.i] = x + o * 0.5;
        self.i += 1;
        if self.i >= self.len {
            self.i = 0;
        }
        o - x
    }
}

/// Freeverb tunings (44.1 kHz) scaled to 48 kHz; the right channel is spread.
const COMB_LEN: [usize; 8] = [1214, 1293, 1389, 1475, 1547, 1622, 1694, 1759];
const AP_LEN: [usize; 4] = [605, 480, 371, 245];
const SPREAD: usize = 25;

struct Reverb {
    pre: Delay<2048>,
    cl: [Comb<2048>; 8],
    cr: [Comb<2048>; 8],
    al: [Allpass<1024>; 4],
    ar: [Allpass<1024>; 4],
}

impl Reverb {
    const NEW: Self = Self {
        pre: Delay::NEW,
        cl: [Comb::NEW; 8],
        cr: [Comb::NEW; 8],
        al: [Allpass::NEW; 4],
        ar: [Allpass::NEW; 4],
    };

    fn init(&mut self) {
        for k in 0..8 {
            self.cl[k].len = COMB_LEN[k];
            self.cr[k].len = COMB_LEN[k] + SPREAD;
        }
        for k in 0..4 {
            self.al[k].len = AP_LEN[k];
            self.ar[k].len = AP_LEN[k] + SPREAD;
        }
    }

    /// Wet only, stereo, at roughly the input's level.
    #[inline]
    fn process(&mut self, x: f32, room: f32) -> (f32, f32) {
        self.pre.push(x);
        let x = self.pre.read(PREDELAY) * 0.015;
        let mut l = 0.0;
        let mut r = 0.0;
        for c in self.cl.iter_mut() {
            l += c.process(x, room, DAMP);
        }
        for c in self.cr.iter_mut() {
            r += c.process(x, room, DAMP);
        }
        for a in self.al.iter_mut() {
            l = a.process(l);
        }
        for a in self.ar.iter_mut() {
            r = a.process(r);
        }
        (l * 3.0, r * 3.0)
    }
}

// ---------------------------------------------------------------- state

/// The delay lines. They are all zero at rest, so they live in .bss and add
/// nothing to the patch file; `patch_init` zeroes them before use.
struct Bufs {
    wob_line: Delay<128>,
    rev: Reverb,
}

impl Bufs {
    const NEW: Self = Self { wob_line: Delay::NEW, rev: Reverb::NEW };
}

struct Mb {
    ready: bool,
    error: bool,
    // knobs, 0..1
    drive: f32,
    room: f32,
    level: f32,
    loud: bool,
    bigsby: bool,
    // smoothed values and targets
    g_pre: f32,
    g_pre_t: f32,
    out_g: f32,
    out_t: f32,
    room_s: f32,
    mix: f32,
    // Ocean filters
    pre_hp: Biquad,
    inter_hp: Biquad,
    inter_lp: Biquad,
    tone_low: Biquad,
    tone_mid: Biquad,
    tone_treb: Biquad,
    tone_dc: Biquad,
    // Peace filters
    peace_hp: Biquad,
    peace_mid: Biquad,
    peace_treb: Biquad,
    hp_in: Biquad,
    cab: [Biquad; 6],
    fb_bp: Biquad,
    u1: f32,
    u2: f32,
    up: f32,
    // dynamics
    env: f32,
    env_slow: f32,
    sag: f32,
    since_onset: u32,
    fb_on: bool,
    fb_ref: f32,
    fb_ramp: f32,
    fb_ph: f32,
    fb_count: u32,
    // Bigsby
    wob_ph: f32,
    wob_drift_ph: f32,
    wob_sway_ph: f32,
    wob_amt: f32,
    // coefficients
    c_param: f32,
    c_mix: f32,
    c_att: f32,
    c_rel: f32,
    c_slow: f32,
    c_sag_att: f32,
    c_sag_rel: f32,
    c_fb_up: f32,
    c_fb_down: f32,
    c_hand: f32,
}

impl Mb {
    const NEW: Self = Self {
        ready: false,
        error: false,
        drive: 0.75,
        room: 0.45,
        level: 0.75,
        loud: true,
        bigsby: false,
        g_pre: 1.0,
        g_pre_t: 1.0,
        out_g: 0.0,
        out_t: 0.0,
        room_s: 0.45,
        mix: 1.0,
        pre_hp: Biquad::NEW,
        inter_hp: Biquad::NEW,
        inter_lp: Biquad::NEW,
        tone_low: Biquad::NEW,
        tone_mid: Biquad::NEW,
        tone_treb: Biquad::NEW,
        tone_dc: Biquad::NEW,
        peace_hp: Biquad::NEW,
        peace_mid: Biquad::NEW,
        peace_treb: Biquad::NEW,
        hp_in: Biquad::NEW,
        cab: [Biquad::NEW; 6],
        fb_bp: Biquad::NEW,
        u1: 0.0,
        u2: 0.0,
        up: 0.0,
        env: 0.0,
        env_slow: 0.0,
        sag: 0.0,
        since_onset: 0,
        fb_on: false,
        fb_ref: 0.0,
        fb_ramp: 0.0,
        fb_ph: 0.0,
        fb_count: 0,
        wob_ph: 0.0,
        wob_drift_ph: 0.0,
        wob_sway_ph: 0.5,
        wob_amt: 0.0,
        c_param: 1.0,
        c_mix: 1.0,
        c_att: 1.0,
        c_rel: 1.0,
        c_slow: 1.0,
        c_sag_att: 1.0,
        c_sag_rel: 1.0,
        c_fb_up: 1.0,
        c_fb_down: 1.0,
        c_hand: 1.0,
    };

    fn init(&mut self, b: &mut Bufs) {
        self.hp_in.highpass(70.0, 0.707);
        // Ocean: loose lows (a tweed doesn't tighten anything up) ...
        self.pre_hp.highpass(60.0, 0.707);
        self.inter_hp.highpass(40.0, 0.707); // also strips stage 1's DC
        self.inter_lp.lowpass(5500.0, 0.707);
        // ... woolly low mids, a mid push and a bright top.
        self.tone_low.peak(180.0, 0.8, 2.5);
        self.tone_mid.peak(900.0, 0.7, 3.0);
        self.tone_treb.peak(3000.0, 0.8, 2.0);
        self.tone_dc.highpass(30.0, 0.707);
        // Peace: the same amp turned down.
        self.peace_hp.highpass(70.0, 0.707);
        self.peace_mid.peak(800.0, 0.7, 2.0);
        self.peace_treb.peak(3000.0, 0.8, 2.5);
        // Open-back 1x12, Jensen-style: lean bottom, honky upper mids, early
        // roll-off.
        self.cab[0].highpass(70.0, 0.7);
        self.cab[1].peak(200.0, 1.0, 2.0);
        self.cab[2].peak(1200.0, 1.0, 2.0);
        self.cab[3].peak(2800.0, 1.5, 3.0);
        self.cab[4].lowpass(5000.0, 0.7);
        self.cab[5].lowpass(7500.0, 0.6);
        self.fb_bp.bandpass(FB_FREQ, FB_Q);
        b.rev.init();
        self.c_param = coef(0.02);
        self.c_mix = coef(0.012);
        self.c_att = coef(0.001);
        self.c_rel = coef(0.05);
        self.c_slow = coef(0.03);
        self.c_sag_att = coef(SAG_ATT_S);
        self.c_sag_rel = coef(SAG_REL_S);
        self.c_fb_up = coef(FB_BLOOM_S);
        self.c_fb_down = coef(FB_DIE_S);
        self.c_hand = coef(WOB_HAND_S);
        self.retarget();
        self.g_pre = self.g_pre_t;
        self.out_g = self.out_t;
        self.room_s = self.room;
        self.mix = if self.loud { 1.0 } else { 0.0 };
        self.ready = true;
    }

    fn retarget(&mut self) {
        self.g_pre_t = db(DRIVE_MIN_DB + DRIVE_RANGE_DB * self.drive);
        // Square law: 0 at minimum, unity at 3 o'clock, +5 dB at maximum.
        let v = self.level / 0.75;
        self.out_t = v * v * OUT_TRIM;
    }

    /// Bigsby: a short delay line swept unevenly, so the pitch wavers flat
    /// and back. The signal always goes through it (2 samples at rest), so
    /// switching it on and off never combs.
    #[inline]
    fn bigsby(&mut self, line: &mut Delay<128>, x: f32) -> f32 {
        let t = if self.bigsby { 1.0 } else { 0.0 };
        self.wob_amt += (t - self.wob_amt) * self.c_hand;
        self.wob_drift_ph = wrap(self.wob_drift_ph + WOB_DRIFT_HZ / SR);
        self.wob_sway_ph = wrap(self.wob_sway_ph + WOB_SWAY_HZ / SR);
        let rate = WOB_RATE * (1.0 + WOB_DRIFT * sin(TAU * self.wob_drift_ph));
        self.wob_ph = wrap(self.wob_ph + rate / SR);
        let depth = WOB_DEPTH * self.wob_amt * (1.0 - WOB_SWAY + WOB_SWAY * sin(TAU * self.wob_sway_ph));
        line.push(x);
        line.read(WOB_BASE + depth * 0.5 * (1.0 - cos(TAU * self.wob_ph)))
    }

    /// Ocean voice: two preamp stages -> tweed tone -> sagging power amp (mono).
    #[inline]
    fn ocean(&mut self, x: f32) -> f32 {
        let x = self.pre_hp.process(x);
        // Stage 1 (the volume knob on 10): cheap 2x oversampling.
        let u = x * self.g_pre;
        let y = 0.5 * (stage(0.5 * (u + self.u1), BIAS1) + stage(u, BIAS1));
        self.u1 = u;
        let y = self.inter_lp.process(self.inter_hp.process(y));
        // Stage 2.
        let u = y * db(DRIVE2_DB);
        let y = 0.5 * (stage(0.5 * (u + self.u2), BIAS2) + stage(u, BIAS2));
        self.u2 = u;
        let y = self.tone_treb.process(self.tone_mid.process(self.tone_low.process(y)));
        let y = self.tone_dc.process(y);
        // Power amp on a sagging rectifier: a hard hit pulls the supply
        // down, squashing the attack, and the note swells back as it recovers.
        let a = y.abs();
        self.sag += (a - self.sag) * if a > self.sag { self.c_sag_att } else { self.c_sag_rel };
        let p = y * POWER_DRIVE / (1.0 + SAG * self.sag);
        stage(p, POWER_BIAS) * (1.0 / POWER_DRIVE) * 1.5
    }

    /// Peace voice: the same amp at a low volume (mono).
    #[inline]
    fn peace(&mut self, x: f32) -> f32 {
        let x = self.peace_hp.process(x);
        let u = x * db(PEACE_DRIVE_DB);
        let y = 0.5 * (stage(0.5 * (u + self.up), BIAS1) + stage(u, BIAS1));
        self.up = u;
        self.peace_treb.process(self.peace_mid.process(y))
    }

    #[inline]
    fn tick(&mut self, b: &mut Bufs, x: f32) -> (f32, f32) {
        self.g_pre += (self.g_pre_t - self.g_pre) * self.c_param;
        self.out_g += (self.out_t - self.out_g) * self.c_param;
        self.room_s += (self.room - self.room_s) * self.c_param;
        let mix_t = if self.loud { 1.0 } else { 0.0 };
        self.mix += (mix_t - self.mix) * self.c_mix;

        let x = self.hp_in.process(x);
        let a = x.abs();
        self.env += (a - self.env) * if a > self.env { self.c_att } else { self.c_rel };
        self.env_slow += (self.env - self.env_slow) * self.c_slow;
        if self.env > ONSET * self.env_slow && self.env > ONSET_MIN {
            self.since_onset = 0;
        } else {
            self.since_onset = self.since_onset.saturating_add(1);
        }

        // The Bigsby is on the guitar, so it moves both voices.
        let x = self.bigsby(&mut b.wob_line, x);

        // Feedback: once a note is left ringing, the amp takes it over (see
        // FB_WAIT). Picking again, or muting, kills it.
        self.fb_ph = wrap(self.fb_ph + FB_RATE / SR);
        self.fb_count = self.fb_count.wrapping_add(1);
        if self.fb_count & 31 == 0 {
            self.fb_bp.bandpass(FB_FREQ * exp2(FB_WANDER * sin(TAU * self.fb_ph)), FB_Q);
        }
        let h = self.fb_bp.process(x);
        if self.since_onset == 0 || self.env < FB_MUTE || !self.loud {
            self.fb_on = false;
        } else if !self.fb_on && self.since_onset as f32 > FB_WAIT * SR && self.env > FB_HOLD {
            self.fb_on = true;
            self.fb_ref = self.env;
        }
        let ft = if self.fb_on { 1.0 } else { 0.0 };
        self.fb_ramp += (ft - self.fb_ramp) * if ft > self.fb_ramp { self.c_fb_up } else { self.c_fb_down };
        let hold = (self.fb_ref / self.env.max(1e-6)).min(db(FB_MAX_DB));
        let xs = x + (hold * (FB_DRY * x + FB_HARM * h) - x) * self.fb_ramp;

        // Both voices always run, so switching is a click-free crossfade.
        let o = self.ocean(xs);
        let p = self.peace(x) * db(PEACE_LEVEL_DB + PEACE_MAKEUP_DB);
        let m = self.mix;
        let mut y = p + (o - p) * m;
        for c in self.cab.iter_mut() {
            y = c.process(y);
        }

        // Shared room, so its tail carries across a switch.
        let rs = self.room_s;
        let (wl, wr) = b.rev.process(y, ROOM_MIN + ROOM_RANGE * rs);
        let wet = rs * (PEACE_WET + (OCEAN_WET - PEACE_WET) * m);
        (limit((y + wet * wl) * self.out_g), limit((y + wet * wr) * self.out_g))
    }

    fn led(&self) -> Color {
        if self.error {
            return Color::Red;
        }
        match (self.loud, self.bigsby) {
            (false, false) => Color::LightGreen,
            (false, true) => Color::LightYellow,
            (true, false) => Color::DarkCobalt,
            (true, true) => Color::Beige,
        }
    }
}

/// Phase wrap into 0..1.
#[inline]
fn wrap(p: f32) -> f32 {
    if p >= 1.0 { p - 1.0 } else { p }
}

/// Preamp stage: asymmetric tube-style saturation, zero at rest.
#[inline]
fn stage(u: f32, bias: f32) -> f32 {
    tanh(u + bias) - tanh(bias)
}

/// Output soft limiter, never reaching ±1.
#[inline]
fn limit(x: f32) -> f32 {
    tanh(x) * 0.98
}

/// All patch state. The firmware calls every entry point from the audio
/// thread (SDK: "no need to synchronize").
struct Global<T>(UnsafeCell<T>);
unsafe impl<T> Sync for Global<T> {}
static STATE: Global<Mb> = Global(UnsafeCell::new(Mb::NEW));
static BUFS: Global<Bufs> = Global(UnsafeCell::new(Bufs::NEW));

#[allow(clippy::mut_from_ref)]
fn state() -> &'static mut Mb {
    // SAFETY: single-threaded access from the firmware's audio thread.
    unsafe { &mut *STATE.0.get() }
}

#[allow(clippy::mut_from_ref)]
fn bufs() -> &'static mut Bufs {
    // SAFETY: as `state`.
    unsafe { &mut *BUFS.0.get() }
}

/// Zeroes the delay lines in place. The loader should already have cleared
/// .bss, but nothing promises it, and a NaN left in a feedback loop never
/// decays. (In place: a zeroed copy would be a large stack temporary.)
fn clear_bufs() {
    // SAFETY: `Bufs` is only f32 and usize fields, so all-zero bits are a
    // valid value; single-threaded as `state`.
    unsafe { core::ptr::write_bytes(BUFS.0.get(), 0, 1) }
}

// ---------------------------------------------------------------- entry points

const PARAM_DRIVE: i32 = 0;
const PARAM_ROOM: i32 = 1;
const PARAM_LEVEL: i32 = 2;

unsafe extern "C" fn patch_init(env: *const PatchEnv) {
    if env.is_null() || unsafe { (*env).abi_version } != PATCH_ABI_VERSION as u32 {
        return;
    }
    clear_bufs();
    state().init(bufs());
}

unsafe extern "C" fn update_buffers(_: *const PatchEnv, left: *mut f32, right: *mut f32, samples: i32) -> f32 {
    let Ok(n) = usize::try_from(samples) else { return 0.0 };
    if left.is_null() || right.is_null() || n == 0 {
        return 0.0;
    }
    // SAFETY: the firmware passes two buffers of `samples` floats.
    let (l, r) = unsafe { (core::slice::from_raw_parts_mut(left, n), core::slice::from_raw_parts_mut(right, n)) };
    let (s, bf) = (state(), bufs());
    if !s.ready {
        return 0.0;
    }
    for (a, b) in l.iter_mut().zip(r.iter_mut()) {
        let (ol, or) = s.tick(bf, *a);
        *a = ol;
        *b = or;
    }
    0.0
}

unsafe extern "C" fn set_buffer(_: *const PatchEnv, buffer: *mut f32) {
    // All state is static; the working buffer isn't needed.
    if buffer.is_null() {
        state().error = true;
    }
}

unsafe extern "C" fn get_buffer_size(_: *const PatchEnv) -> i32 {
    WORKING_BUFFER_SIZE
}

unsafe extern "C" fn get_param_min(_: *const PatchEnv, _: i32) -> f32 {
    0.0
}

unsafe extern "C" fn get_param_max(_: *const PatchEnv, _: i32) -> f32 {
    1.0
}

unsafe extern "C" fn get_param_default(_: *const PatchEnv, idx: i32) -> f32 {
    match idx {
        PARAM_DRIVE => 0.75,
        PARAM_ROOM => 0.45,
        _ => 0.75,
    }
}

unsafe extern "C" fn is_param_enabled(_: *const PatchEnv, idx: i32, source_id: i32) -> u8 {
    // Knobs only (source 0), like the SDK wrapper.
    (source_id == 0 && (0..PARAM_COUNT).contains(&idx)) as u8
}

unsafe fn write_c_str(s: &str, out: *mut u8, len: usize) {
    if out.is_null() || len == 0 {
        return;
    }
    let n = s.len().min(len - 1);
    // SAFETY: the firmware provides `len` writable bytes at `out`.
    unsafe {
        core::ptr::copy_nonoverlapping(s.as_ptr(), out, n);
        *out.add(n) = 0;
    }
}

unsafe extern "C" fn get_param_name(_: *const PatchEnv, idx: i32, out: *mut u8, len: usize) {
    let name = match idx {
        PARAM_DRIVE => "Drive",
        PARAM_ROOM => "Room",
        PARAM_LEVEL => "Level",
        _ => "",
    };
    unsafe { write_c_str(name, out, len) }
}

unsafe extern "C" fn get_param_unit(_: *const PatchEnv, _: i32, out: *mut u8, len: usize) {
    unsafe { write_c_str("", out, len) }
}

unsafe extern "C" fn set_param(_: *const PatchEnv, idx: i32, value: f32) {
    let s = state();
    let v = value.clamp(0.0, 1.0);
    match idx {
        PARAM_DRIVE => s.drive = v,
        PARAM_ROOM => s.room = v,
        PARAM_LEVEL => s.level = v,
        _ => return,
    }
    if s.ready {
        s.retarget();
    }
}

unsafe extern "C" fn special_action(_: *const PatchEnv, idx: i32) {
    let s = state();
    match idx {
        ACTION_FOOTSWITCH_PRESS => s.loud = !s.loud,
        ACTION_FOOTSWITCH_HOLD => s.bigsby = !s.bigsby,
        _ => return,
    }
    if s.ready {
        s.retarget();
    }
}

unsafe extern "C" fn get_state_idx(_: *const PatchEnv) -> i32 {
    state().led() as i32
}

// ---------------------------------------------------------------- header

#[cfg(target_os = "none")]
unsafe extern "C" {
    static __patch_bss_start: u8;
    static patch_image_size: u8;
    static patch_bss_size: u8;
}

#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".patch_header")]
#[used]
pub static patch_header: PatchHeader = PatchHeader {
    magic: PATCH_MAGIC,
    abi_version: PATCH_ABI_VERSION,
    flags: PATCH_FLAG_NONE,
    init: patch_init,
    agent_update_buffers: update_buffers,
    agent_set_buffer: set_buffer,
    agent_get_buffer_size: get_buffer_size,
    agent_get_param_min: get_param_min,
    agent_get_param_max: get_param_max,
    agent_get_param_default: get_param_default,
    agent_is_param_enabled: is_param_enabled,
    agent_get_param_name: get_param_name,
    agent_get_param_unit: get_param_unit,
    agent_set_param: set_param,
    agent_special_action: special_action,
    agent_get_state_idx: get_state_idx,
    image_size: &raw const patch_image_size,
    bss_begin: &raw const __patch_bss_start,
    bss_size: &raw const patch_bss_size,
    reserved: [0; 16],
};

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(target_os = "none"))]
#[allow(dead_code)] // each demo uses only some of the playing techniques
mod guitar;
#[cfg(not(target_os = "none"))]
mod host;

#[cfg(not(target_os = "none"))]
fn main() {
    host::main();
}
