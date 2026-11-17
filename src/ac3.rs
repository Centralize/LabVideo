//! Dolby Digital (AC-3) audio decoder, written from the public ATSC A/52 standard.
//!
//! Windows no longer ships an AC-3 decoder, so the player decodes AC-3 itself, like the original
//! does with its own codec library. The tables below are the standard's; each was checked against
//! the copy inside the original's codec library (addresses in MECHANICS.md).
//!
//! Output is interleaved float PCM: mono, stereo, or 6 channels in Windows order
//! (front left, front right, centre, LFE, surround left, surround right).

use std::f64::consts::PI;

pub const SAMPLE_RATES: [u32; 3] = [48000, 44100, 32000];
const BITRATES: [usize; 19] = [32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640];
/// Samples per channel in one frame (6 blocks of 256).
pub const FRAME_SAMPLES: usize = 1536;

/// Hearing threshold per band, for 48 / 44.1 / 32 kHz.
const HTH: [[u16; 3]; 50] = [
    [0x04d0, 0x04f0, 0x0580], [0x04d0, 0x04f0, 0x0580], [0x0440, 0x0460, 0x04b0], [0x0400, 0x0410, 0x0450],
    [0x03e0, 0x03e0, 0x0420], [0x03c0, 0x03d0, 0x03f0], [0x03b0, 0x03c0, 0x03e0], [0x03b0, 0x03b0, 0x03d0],
    [0x03a0, 0x03b0, 0x03c0], [0x03a0, 0x03a0, 0x03b0], [0x03a0, 0x03a0, 0x03b0], [0x03a0, 0x03a0, 0x03b0],
    [0x03a0, 0x03a0, 0x03a0], [0x0390, 0x03a0, 0x03a0], [0x0390, 0x0390, 0x03a0], [0x0390, 0x0390, 0x03a0],
    [0x0380, 0x0390, 0x03a0], [0x0380, 0x0380, 0x03a0], [0x0370, 0x0380, 0x03a0], [0x0370, 0x0380, 0x03a0],
    [0x0360, 0x0370, 0x0390], [0x0360, 0x0370, 0x0390], [0x0350, 0x0360, 0x0390], [0x0350, 0x0360, 0x0390],
    [0x0340, 0x0350, 0x0380], [0x0340, 0x0350, 0x0380], [0x0330, 0x0340, 0x0380], [0x0320, 0x0340, 0x0370],
    [0x0310, 0x0320, 0x0360], [0x0300, 0x0310, 0x0350], [0x02f0, 0x0300, 0x0340], [0x02f0, 0x02f0, 0x0330],
    [0x02f0, 0x02f0, 0x0320], [0x02f0, 0x02f0, 0x0310], [0x0300, 0x02f0, 0x0300], [0x0310, 0x0300, 0x02f0],
    [0x0340, 0x0320, 0x02f0], [0x0390, 0x0350, 0x02f0], [0x03e0, 0x0390, 0x0300], [0x0420, 0x03e0, 0x0310],
    [0x0460, 0x0420, 0x0330], [0x0490, 0x0450, 0x0350], [0x04a0, 0x04a0, 0x03c0], [0x0460, 0x0490, 0x0410],
    [0x0440, 0x0460, 0x0470], [0x0440, 0x0440, 0x04a0], [0x0520, 0x0480, 0x0460], [0x0800, 0x0630, 0x0440],
    [0x0840, 0x0840, 0x0450], [0x0840, 0x0840, 0x04e0],
];

/// Log-addition table.
const LATAB: [u8; 260] = [
    0x40, 0x3f, 0x3e, 0x3d, 0x3c, 0x3b, 0x3a, 0x39, 0x38, 0x37, 0x36, 0x35, 0x34, 0x34, 0x33, 0x32, 0x31, 0x30, 0x2f, 0x2f,
    0x2e, 0x2d, 0x2c, 0x2c, 0x2b, 0x2a, 0x29, 0x29, 0x28, 0x27, 0x26, 0x26, 0x25, 0x24, 0x24, 0x23, 0x23, 0x22, 0x21, 0x21,
    0x20, 0x20, 0x1f, 0x1e, 0x1e, 0x1d, 0x1d, 0x1c, 0x1c, 0x1b, 0x1b, 0x1a, 0x1a, 0x19, 0x19, 0x18, 0x18, 0x17, 0x17, 0x16,
    0x16, 0x15, 0x15, 0x15, 0x14, 0x14, 0x13, 0x13, 0x13, 0x12, 0x12, 0x12, 0x11, 0x11, 0x11, 0x10, 0x10, 0x10, 0x0f, 0x0f,
    0x0f, 0x0e, 0x0e, 0x0e, 0x0d, 0x0d, 0x0d, 0x0d, 0x0c, 0x0c, 0x0c, 0x0c, 0x0b, 0x0b, 0x0b, 0x0b, 0x0a, 0x0a, 0x0a, 0x0a,
    0x0a, 0x09, 0x09, 0x09, 0x09, 0x09, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x06, 0x06,
    0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04,
    0x04, 0x04, 0x04, 0x04, 0x04, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x02,
    0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// First bin of each of the 50 bit-allocation bands (and the end, 253).
const BAND_START: [usize; 51] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 31, 34, 37,
    40, 43, 46, 49, 55, 61, 67, 73, 79, 85, 97, 109, 121, 133, 157, 181, 205, 229, 253,
];

/// Bit-allocation pointer from the masked power.
const BAPTAB: [u8; 64] = [
    0, 1, 1, 1, 1, 1, 2, 2, 3, 3, 3, 4, 4, 5, 5, 6, 6, 6, 6, 7, 7, 7, 7, 8, 8, 8, 8, 9, 9, 9, 9, 10, 10, 10, 10, 11, 11, 11,
    11, 12, 12, 12, 12, 13, 13, 13, 13, 14, 14, 14, 14, 14, 14, 14, 14, 15, 15, 15, 15, 15, 15, 15, 15, 15,
];
const SLOW_DECAY: [i32; 4] = [0x0f, 0x11, 0x13, 0x15];
const FAST_DECAY: [i32; 4] = [0x3f, 0x53, 0x67, 0x7b];
const SLOW_GAIN: [i32; 4] = [0x540, 0x4d8, 0x478, 0x410];
const DB_PER_BIT: [i32; 4] = [0x000, 0x700, 0x900, 0xb00];
const FLOOR: [i32; 8] = [0x2f0, 0x2b0, 0x270, 0x230, 0x1f0, 0x170, 0x0f0, -0x800];
const FAST_GAIN: [i32; 8] = [0x080, 0x100, 0x180, 0x200, 0x280, 0x300, 0x380, 0x400];
/// Mantissa bits for bit-allocation pointers 6..15 (asymmetric quantizers).
const QBITS: [u32; 16] = [0, 0, 0, 0, 0, 0, 5, 6, 7, 8, 9, 10, 11, 12, 14, 16];
/// Rematrixing band edges (stereo).
const REMAT_BANDS: [usize; 5] = [13, 25, 37, 61, 253];
/// Full-bandwidth channels for each audio coding mode.
const NFCHANS: [usize; 8] = [2, 1, 2, 3, 3, 4, 4, 5];

const CPL: usize = 5;
const LFE: usize = 6;

fn masktab() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut band = 0;
    for (bin, v) in t.iter_mut().enumerate() {
        while band + 1 < 50 && bin >= BAND_START[band + 1] {
            band += 1;
        }
        *v = band as u8;
    }
    t
}

/// Frame length in bytes, from the sample-rate code and frame-size code.
pub fn frame_bytes(fscod: usize, frmsizecod: usize) -> Option<usize> {
    if fscod > 2 || frmsizecod > 37 {
        return None;
    }
    let br = BITRATES[frmsizecod / 2];
    let words = match fscod {
        0 => br * 2,
        1 => br * 320 / 147 + (frmsizecod & 1),
        _ => br * 3,
    };
    Some(words * 2)
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn read(&mut self, n: u32) -> u32 {
        let mut v = 0u32;
        for _ in 0..n {
            let byte = self.data.get(self.pos >> 3).copied().unwrap_or(0);
            v = (v << 1) | ((byte >> (7 - (self.pos & 7))) & 1) as u32;
            self.pos += 1;
        }
        v
    }
    fn flag(&mut self) -> bool {
        self.read(1) == 1
    }
    fn signed(&mut self, n: u32) -> i32 {
        let v = self.read(n) as i32;
        let s = 32 - n;
        (v << s) >> s
    }
    fn skip(&mut self, n: usize) {
        self.pos += n;
    }
    fn overrun(&self) -> bool {
        self.pos > self.data.len() * 8
    }
}

/// Mantissas grouped three (or two) to a code share their code across channels in a block.
#[derive(Default)]
struct Groups {
    q1: [f32; 2],
    n1: usize,
    q2: [f32; 2],
    n2: usize,
    q4: f32,
    n4: usize,
}

fn sym(level: i32, levels: i32) -> f32 {
    (2 * level - (levels - 1)) as f32 / levels as f32
}

/// One decoded frame.
pub struct Frame {
    pub rate: u32,
    pub channels: usize,
    pub samples: Vec<f32>,
}

pub struct Ac3Decoder {
    window: Vec<f32>,
    tab_long: Vec<f32>,
    tab_short1: Vec<f32>,
    tab_short2: Vec<f32>,
    delay: [[f32; 256]; 6],
    masktab: [u8; 256],
    rng: u32,
}

impl Default for Ac3Decoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Kaiser-Bessel-derived window, alpha 5, 512 points (the standard's transform window).
pub fn kbd_window() -> Vec<f32> {
    fn bessel_i0(x: f64) -> f64 {
        let mut sum = 1.0;
        let mut term = 1.0;
        for k in 1..60 {
            term *= (x / 2.0) / k as f64;
            sum += term * term;
        }
        sum
    }
    let alpha = 5.0;
    let kernel: Vec<f64> = (0..=256)
        .map(|j| {
            let r = (j as f64 - 128.0) / 128.0;
            bessel_i0(PI * alpha * (1.0 - r * r).max(0.0).sqrt())
        })
        .collect();
    let total: f64 = kernel.iter().sum();
    let mut w = vec![0f32; 512];
    let mut acc = 0.0;
    for n in 0..256 {
        acc += kernel[n];
        let v = (acc / total).sqrt() as f32;
        w[n] = v;
        w[511 - n] = v;
    }
    w
}

/// Transform kernel cos(2π/(4N)·(2n+1)(2k+1) + π/4·(2k+1)(1+α)) for N-point blocks.
pub fn kernel(n_len: usize, alpha: f64, n: usize, k: usize) -> f64 {
    let nn = n_len as f64;
    (2.0 * PI / (4.0 * nn) * (2 * n + 1) as f64 * (2 * k + 1) as f64 + PI / 4.0 * (2 * k + 1) as f64 * (1.0 + alpha)).cos()
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0f32; 8];
    let chunks = a.len() / 8;
    for c in 0..chunks {
        for l in 0..8 {
            acc[l] += a[c * 8 + l] * b[c * 8 + l];
        }
    }
    let mut s: f32 = acc.iter().sum();
    for i in chunks * 8..a.len() {
        s += a[i] * b[i];
    }
    s
}

impl Ac3Decoder {
    pub fn new() -> Self {
        let mut tab_long = vec![0f32; 512 * 256];
        for n in 0..512 {
            for k in 0..256 {
                tab_long[n * 256 + k] = kernel(512, 0.0, n, k) as f32;
            }
        }
        let mut tab_short1 = vec![0f32; 256 * 128];
        let mut tab_short2 = vec![0f32; 256 * 128];
        for n in 0..256 {
            for k in 0..128 {
                tab_short1[n * 128 + k] = kernel(256, -1.0, n, k) as f32;
                tab_short2[n * 128 + k] = kernel(256, 1.0, n, k) as f32;
            }
        }
        Self { window: kbd_window(), tab_long, tab_short1, tab_short2, delay: [[0.0; 256]; 6], masktab: masktab(), rng: 0x1234_5678 }
    }

    pub fn reset(&mut self) {
        self.delay = [[0.0; 256]; 6];
    }

    fn dither(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        // uniform in [-0.5, 0.5)
        ((self.rng >> 8) as f32 / (1u32 << 24) as f32) - 0.5
    }

    /// Inverse transform of one block, windowed and overlapped with the previous one.
    fn imdct(&self, x: &[f32; 256], short: bool, delay: &mut [f32; 256], out: &mut [f32; 256]) {
        let mut y = [0f32; 512];
        if !short {
            for (n, v) in y.iter_mut().enumerate() {
                *v = dot(&self.tab_long[n * 256..n * 256 + 256], x);
            }
        } else {
            let mut x1 = [0f32; 128];
            let mut x2 = [0f32; 128];
            for k in 0..128 {
                x1[k] = x[2 * k];
                x2[k] = x[2 * k + 1];
            }
            for n in 0..256 {
                y[n] = dot(&self.tab_short1[n * 128..n * 128 + 128], &x1);
                y[256 + n] = dot(&self.tab_short2[n * 128..n * 128 + 128], &x2);
            }
        }
        // the forward transform is scaled by -2/N, so the inverse output is scaled by -2
        for n in 0..512 {
            y[n] *= self.window[n] * -2.0;
        }
        for n in 0..256 {
            out[n] = y[n] + delay[n];
            delay[n] = y[256 + n];
        }
    }

    fn mantissa(&mut self, b: &mut Bits, g: &mut Groups, bap: u8, dither: bool) -> f32 {
        match bap {
            0 => {
                if dither {
                    self.dither()
                } else {
                    0.0
                }
            }
            1 => {
                if g.n1 > 0 {
                    let v = g.q1[2 - g.n1];
                    g.n1 -= 1;
                    v
                } else {
                    let code = b.read(5) as i32;
                    g.q1 = [sym((code % 9) / 3, 3), sym(code % 3, 3)];
                    g.n1 = 2;
                    sym(code / 9, 3)
                }
            }
            2 => {
                if g.n2 > 0 {
                    let v = g.q2[2 - g.n2];
                    g.n2 -= 1;
                    v
                } else {
                    let code = b.read(7) as i32;
                    g.q2 = [sym((code % 25) / 5, 5), sym(code % 5, 5)];
                    g.n2 = 2;
                    sym(code / 25, 5)
                }
            }
            3 => {
                let v = b.read(3) as i32;
                if v < 7 {
                    sym(v, 7)
                } else {
                    0.0
                }
            }
            4 => {
                if g.n4 > 0 {
                    g.n4 = 0;
                    g.q4
                } else {
                    let code = b.read(7) as i32;
                    g.q4 = sym(code % 11, 11);
                    g.n4 = 1;
                    sym(code / 11, 11)
                }
            }
            5 => {
                let v = b.read(4) as i32;
                if v < 15 {
                    sym(v, 15)
                } else {
                    0.0
                }
            }
            _ => {
                let bits = QBITS[(bap as usize).min(15)];
                b.signed(bits) as f32 / (1u32 << (bits - 1)) as f32
            }
        }
    }

    /// Decode one whole AC-3 frame (sync word first). None if it isn't a frame this decoder reads.
    pub fn decode_frame(&mut self, frame: &[u8]) -> Option<Frame> {
        let mut b = Bits::new(frame);
        if b.read(16) != 0x0B77 {
            return None;
        }
        b.read(16); // crc1
        let fscod = b.read(2) as usize;
        let frmsizecod = b.read(6) as usize;
        let size = frame_bytes(fscod, frmsizecod)?;
        if frame.len() < size {
            return None;
        }
        let bsid = b.read(5);
        if bsid > 10 {
            return None; // E-AC-3 and newer formats
        }
        b.read(3); // bsmod
        let acmod = b.read(3) as usize;
        if acmod & 1 != 0 && acmod != 1 {
            b.read(2); // cmixlev
        }
        if acmod & 4 != 0 {
            b.read(2); // surmixlev
        }
        if acmod == 2 {
            b.read(2); // dsurmod
        }
        let lfeon = b.flag();
        for _ in 0..if acmod == 0 { 2 } else { 1 } {
            b.read(5); // dialnorm
            if b.flag() {
                b.read(8); // compr
            }
            if b.flag() {
                b.read(8); // langcod
            }
            if b.flag() {
                b.read(7); // mixlevel, roomtyp
            }
        }
        b.read(2); // copyrightb, origbs
        if b.flag() {
            b.read(14);
        }
        if b.flag() {
            b.read(14);
        }
        if b.flag() {
            let l = b.read(6) as usize;
            b.skip((l + 1) * 8);
        }
        let mut rate = SAMPLE_RATES[fscod];
        if bsid == 9 {
            rate /= 2;
        } else if bsid == 10 {
            rate /= 4;
        }
        let nf = NFCHANS[acmod];

        // output layout: where each coded channel goes
        let six = acmod >= 3 || lfeon;
        let out_ch = if six { 6 } else if acmod == 1 { 1 } else { 2 };
        let slots: &[&[usize]] = match (acmod, six) {
            (1, false) => &[&[0]],
            (1, true) => &[&[2]],
            (0, _) | (2, _) => &[&[0], &[1]],
            (3, _) => &[&[0], &[2], &[1]],
            (4, _) => &[&[0], &[1], &[4, 5]],
            (5, _) => &[&[0], &[2], &[1], &[4, 5]],
            (6, _) => &[&[0], &[1], &[4], &[5]],
            _ => &[&[0], &[2], &[1], &[4], &[5]],
        };
        let mut samples = vec![0f32; FRAME_SAMPLES * out_ch];

        // state carried from block to block within the frame
        let mut blksw = [false; 5];
        let mut dithflag = [false; 5];
        let mut dyn_gain = [1.0f32; 2];
        let mut cplinu = false;
        let mut chincpl = [false; 5];
        let mut phsflginu = false;
        let mut cplbegf = 0usize;
        let mut ncplsubnd = 0usize;
        let mut ncplbnd = 0usize;
        let mut cplbndstrc = [false; 18];
        let mut cplstrtmant = 0usize;
        let mut cplendmant = 0usize;
        let mut cplco = [[1.0f32; 18]; 5];
        let mut phsflg = [false; 18];
        let mut rematflg = [false; 4];
        let mut exps = [[0u8; 256]; 7];
        let mut bap = [[0u8; 256]; 7];
        let mut chbwcod = [0usize; 5];
        let mut endmant = [0usize; 7];
        let (mut sdcycod, mut fdcycod, mut sgaincod, mut dbpbcod, mut floorcod) = (0usize, 0usize, 0usize, 0usize, 0usize);
        let mut csnroffst = 0i32;
        let mut fsnroffst = [0i32; 7];
        let mut fgaincod = [0usize; 7];
        let (mut cplfleak, mut cplsleak) = (0i32, 0i32);
        let mut delta: [Vec<(usize, usize, i32)>; 7] = Default::default();

        for blk in 0..6 {
            for s in blksw.iter_mut().take(nf) {
                *s = b.flag();
            }
            for d in dithflag.iter_mut().take(nf) {
                *d = b.flag();
            }
            if b.flag() {
                dyn_gain[0] = dynrng(b.read(8));
            } else if blk == 0 {
                dyn_gain[0] = 1.0;
            }
            if acmod == 0 {
                if b.flag() {
                    dyn_gain[1] = dynrng(b.read(8));
                } else if blk == 0 {
                    dyn_gain[1] = 1.0;
                }
            }
            // coupling strategy
            if b.flag() {
                cplinu = b.flag();
                if cplinu {
                    for c in chincpl.iter_mut().take(nf) {
                        *c = b.flag();
                    }
                    if acmod == 2 {
                        phsflginu = b.flag();
                    }
                    cplbegf = b.read(4) as usize;
                    let cplendf = b.read(4) as usize;
                    if cplendf + 3 < cplbegf {
                        return None;
                    }
                    ncplsubnd = 3 + cplendf - cplbegf;
                    ncplbnd = ncplsubnd;
                    cplbndstrc = [false; 18];
                    for s in 1..ncplsubnd {
                        cplbndstrc[s] = b.flag();
                        if cplbndstrc[s] {
                            ncplbnd -= 1;
                        }
                    }
                    cplstrtmant = cplbegf * 12 + 37;
                    cplendmant = (cplendf + 3) * 12 + 37;
                } else {
                    chincpl = [false; 5];
                }
            }
            // coupling coordinates
            if cplinu {
                let mut any_new = false;
                for ch in 0..nf {
                    if chincpl[ch] && b.flag() {
                        any_new = true;
                        let mstr = 3 * b.read(2) as i32;
                        for bnd in 0..ncplbnd {
                            let e = b.read(4) as i32;
                            let m = b.read(4) as f32;
                            let t = if e == 15 { m / 16.0 } else { (m + 16.0) / 32.0 };
                            cplco[ch][bnd] = t * 2f32.powi(-(e + mstr)) * 8.0;
                        }
                    }
                }
                if acmod == 2 && phsflginu && any_new {
                    for f in phsflg.iter_mut().take(ncplbnd) {
                        *f = b.flag();
                    }
                }
            }
            // rematrixing
            if acmod == 2 && b.flag() {
                let n = if !cplinu || cplbegf > 2 { 4 } else if cplbegf > 0 { 3 } else { 2 };
                for (r, f) in rematflg.iter_mut().enumerate() {
                    *f = r < n && b.flag();
                }
            }
            // exponent strategies
            let cplexpstr = if cplinu { b.read(2) } else { 0 };
            let mut chexpstr = [0u32; 5];
            for s in chexpstr.iter_mut().take(nf) {
                *s = b.read(2);
            }
            let lfeexpstr = if lfeon { b.read(1) } else { 0 };
            for ch in 0..nf {
                if chexpstr[ch] != 0 && !chincpl[ch] {
                    chbwcod[ch] = b.read(6) as usize;
                    if chbwcod[ch] > 60 {
                        return None;
                    }
                }
                endmant[ch] = if cplinu && chincpl[ch] { cplstrtmant } else { (chbwcod[ch] + 12) * 3 + 37 };
            }
            // exponents
            if cplinu {
                endmant[CPL] = cplendmant;
                if cplexpstr != 0 {
                    let absexp = (b.read(4) << 1) as i32;
                    let grp = 1usize << (cplexpstr - 1);
                    let ngrps = (cplendmant - cplstrtmant) / (3 * grp);
                    decode_exps(&mut b, absexp, ngrps, grp, &mut exps[CPL][cplstrtmant..]);
                }
            }
            for ch in 0..nf {
                if chexpstr[ch] != 0 {
                    let grp = 1usize << (chexpstr[ch] - 1);
                    let ngrps = (endmant[ch] - 1 + 3 * grp - 3) / (3 * grp);
                    let absexp = b.read(4) as i32;
                    exps[ch][0] = absexp as u8;
                    decode_exps(&mut b, absexp, ngrps, grp, &mut exps[ch][1..]);
                    b.read(2); // gainrng
                }
            }
            if lfeon {
                endmant[LFE] = 7;
                if lfeexpstr != 0 {
                    let absexp = b.read(4) as i32;
                    exps[LFE][0] = absexp as u8;
                    decode_exps(&mut b, absexp, 2, 1, &mut exps[LFE][1..]);
                }
            }
            // bit allocation parameters
            if b.flag() {
                sdcycod = b.read(2) as usize;
                fdcycod = b.read(2) as usize;
                sgaincod = b.read(2) as usize;
                dbpbcod = b.read(2) as usize;
                floorcod = b.read(3) as usize;
            }
            if b.flag() {
                csnroffst = b.read(6) as i32;
                if cplinu {
                    fsnroffst[CPL] = b.read(4) as i32;
                    fgaincod[CPL] = b.read(3) as usize;
                }
                for ch in 0..nf {
                    fsnroffst[ch] = b.read(4) as i32;
                    fgaincod[ch] = b.read(3) as usize;
                }
                if lfeon {
                    fsnroffst[LFE] = b.read(4) as i32;
                    fgaincod[LFE] = b.read(3) as usize;
                }
            }
            if cplinu && b.flag() {
                cplfleak = b.read(3) as i32;
                cplsleak = b.read(3) as i32;
            }
            if b.flag() {
                let mut mode = [0u32; 7];
                if cplinu {
                    mode[CPL] = b.read(2);
                }
                for m in mode.iter_mut().take(nf) {
                    *m = b.read(2);
                }
                let order: Vec<usize> = if cplinu { std::iter::once(CPL).chain(0..nf).collect() } else { (0..nf).collect() };
                for idx in order {
                    match mode[idx] {
                        1 => {
                            let nseg = b.read(3) as usize + 1;
                            delta[idx] = (0..nseg)
                                .map(|_| {
                                    let off = b.read(5) as usize;
                                    let len = b.read(4) as usize;
                                    let ba = b.read(3) as i32;
                                    (off, len, if ba >= 4 { (ba - 3) << 7 } else { (ba - 4) << 7 })
                                })
                                .collect();
                        }
                        2 => delta[idx].clear(),
                        _ => {}
                    }
                }
            }
            if b.flag() {
                let skipl = b.read(9) as usize;
                b.skip(skipl * 8);
            }
            if b.overrun() {
                return None;
            }

            // bit allocation
            let params = BaParams {
                sdecay: SLOW_DECAY[sdcycod],
                fdecay: FAST_DECAY[fdcycod],
                sgain: SLOW_GAIN[sgaincod],
                dbknee: DB_PER_BIT[dbpbcod],
                floor: FLOOR[floorcod],
                fscod,
            };
            let all_zero = csnroffst == 0
                && (0..nf).all(|c| fsnroffst[c] == 0)
                && (!cplinu || fsnroffst[CPL] == 0)
                && (!lfeon || fsnroffst[LFE] == 0);
            let snr = |f: i32| (((csnroffst - 15) << 4) + f) << 2;
            for ch in 0..nf {
                if all_zero {
                    bap[ch] = [0; 256];
                } else {
                    bit_allocation(&self.masktab, &exps[ch], 0, endmant[ch], &params, FAST_GAIN[fgaincod[ch]], snr(fsnroffst[ch]), None, &delta[ch], &mut bap[ch]);
                }
            }
            if cplinu {
                if all_zero {
                    bap[CPL] = [0; 256];
                } else {
                    let leak = Some(((cplfleak << 8) + 768, (cplsleak << 8) + 768));
                    bit_allocation(&self.masktab, &exps[CPL], cplstrtmant, cplendmant, &params, FAST_GAIN[fgaincod[CPL]], snr(fsnroffst[CPL]), leak, &delta[CPL], &mut bap[CPL]);
                }
            }
            if lfeon {
                if all_zero {
                    bap[LFE] = [0; 256];
                } else {
                    bit_allocation(&self.masktab, &exps[LFE], 0, 7, &params, FAST_GAIN[fgaincod[LFE]], snr(fsnroffst[LFE]), None, &[], &mut bap[LFE]);
                }
            }

            // mantissas
            let mut g = Groups::default();
            let mut coeffs = [[0f32; 256]; 7];
            let mut got_cpl = false;
            for ch in 0..nf {
                for bin in 0..endmant[ch] {
                    let m = self.mantissa(&mut b, &mut g, bap[ch][bin], dithflag[ch]);
                    coeffs[ch][bin] = m * pow2_neg(exps[ch][bin]);
                }
                if cplinu && chincpl[ch] && !got_cpl {
                    for bin in cplstrtmant..cplendmant {
                        let m = self.mantissa(&mut b, &mut g, bap[CPL][bin], false);
                        coeffs[CPL][bin] = m * pow2_neg(exps[CPL][bin]);
                    }
                    got_cpl = true;
                }
            }
            if lfeon {
                for bin in 0..7 {
                    let m = self.mantissa(&mut b, &mut g, bap[LFE][bin], false);
                    coeffs[LFE][bin] = m * pow2_neg(exps[LFE][bin]);
                }
            }
            if b.overrun() {
                return None;
            }

            // decoupling
            if cplinu {
                for ch in 0..nf {
                    if !chincpl[ch] {
                        continue;
                    }
                    let mut bin = cplstrtmant;
                    let mut bnd = 0usize;
                    for s in 0..ncplsubnd {
                        if s > 0 && !cplbndstrc[s] {
                            bnd += 1;
                        }
                        for _ in 0..12 {
                            let mut c = coeffs[CPL][bin];
                            if bap[CPL][bin] == 0 && dithflag[ch] {
                                c = self.dither() * pow2_neg(exps[CPL][bin]);
                            }
                            let mut v = c * cplco[ch][bnd.min(17)];
                            if acmod == 2 && ch == 1 && phsflginu && phsflg[bnd.min(17)] {
                                v = -v;
                            }
                            coeffs[ch][bin] = v;
                            bin += 1;
                        }
                    }
                }
            }
            // rematrixing (stereo sum / difference)
            if acmod == 2 {
                let n = if !cplinu || cplbegf > 2 { 4 } else if cplbegf > 0 { 3 } else { 2 };
                let end_all = if cplinu { cplstrtmant } else { endmant[0].min(endmant[1]) };
                for r in 0..n {
                    if !rematflg[r] {
                        continue;
                    }
                    let end = REMAT_BANDS[r + 1].min(end_all);
                    for bin in REMAT_BANDS[r]..end {
                        let (l, rr) = (coeffs[0][bin], coeffs[1][bin]);
                        coeffs[0][bin] = l + rr;
                        coeffs[1][bin] = l - rr;
                    }
                }
            }
            // dynamic range compression (applied in full, as the original's decoder does by default)
            for ch in 0..nf {
                let gain = if acmod == 0 && ch == 1 { dyn_gain[1] } else { dyn_gain[0] };
                if gain != 1.0 {
                    for c in coeffs[ch].iter_mut() {
                        *c *= gain;
                    }
                }
            }
            if lfeon && dyn_gain[0] != 1.0 {
                for c in coeffs[LFE].iter_mut() {
                    *c *= dyn_gain[0];
                }
            }

            // back to time and into the output
            let mut pcm = [0f32; 256];
            for ch in 0..nf {
                let mut delay = self.delay[ch];
                self.imdct(&coeffs[ch], blksw[ch], &mut delay, &mut pcm);
                self.delay[ch] = delay;
                for &slot in slots[ch] {
                    for (i, v) in pcm.iter().enumerate() {
                        samples[(blk * 256 + i) * out_ch + slot] = *v;
                    }
                }
            }
            if lfeon {
                let mut delay = self.delay[5];
                self.imdct(&coeffs[LFE], false, &mut delay, &mut pcm);
                self.delay[5] = delay;
                for (i, v) in pcm.iter().enumerate() {
                    samples[(blk * 256 + i) * out_ch + 3] = *v;
                }
            }
        }
        Some(Frame { rate, channels: out_ch, samples })
    }
}

fn pow2_neg(e: u8) -> f32 {
    1.0 / (1u32 << e.min(31)) as f32
}

/// Dynamic range word: signed 3-bit power of two and 5-bit fraction, 0 = 0 dB, applied to the
/// power of the decoder's compression scale (1.0 by default: as the stream says).
fn dynrng(v: u32) -> f32 {
    let x = ((v as u8 as i8) >> 5) as i32;
    let y = (v & 0x1f) as f32;
    let gain = 2f32.powi(x) * (32.0 + y) / 32.0;
    let scale = crate::mechanics::AC3_DRC_SCALE;
    if scale == 1.0 {
        gain
    } else {
        gain.powf(scale)
    }
}

fn decode_exps(b: &mut Bits, mut prev: i32, ngrps: usize, grp: usize, out: &mut [u8]) {
    let mut i = 0;
    for _ in 0..ngrps {
        let g = b.read(7) as i32;
        for d in [g / 25, (g % 25) / 5, g % 5] {
            prev += d - 2;
            let v = prev.clamp(0, 24) as u8;
            for _ in 0..grp {
                if i < out.len() {
                    out[i] = v;
                }
                i += 1;
            }
        }
    }
}

pub struct BaParams {
    pub sdecay: i32,
    pub fdecay: i32,
    pub sgain: i32,
    pub dbknee: i32,
    pub floor: i32,
    pub fscod: usize,
}

fn logadd(a: i32, b: i32) -> i32 {
    let c = a - b;
    let addr = ((c.abs() >> 1) as usize).min(255);
    if c >= 0 {
        a + LATAB[addr] as i32
    } else {
        b + LATAB[addr] as i32
    }
}

fn calc_lowcomp(mut a: i32, b0: i32, b1: i32, bin: usize) -> i32 {
    if bin < 7 {
        if b0 + 256 == b1 {
            a = 384;
        } else if b0 > b1 {
            a = (a - 64).max(0);
        }
    } else if bin < 20 {
        if b0 + 256 == b1 {
            a = 320;
        } else if b0 > b1 {
            a = (a - 64).max(0);
        }
    } else {
        a = (a - 128).max(0);
    }
    a
}

/// The standard's parametric bit allocation: exponents → bit-allocation pointers.
#[allow(clippy::too_many_arguments)]
pub fn bit_allocation(
    masktab: &[u8; 256],
    exps: &[u8; 256],
    start: usize,
    end: usize,
    p: &BaParams,
    fgain: i32,
    snroffset: i32,
    cpl_leak: Option<(i32, i32)>,
    delta: &[(usize, usize, i32)],
    bap: &mut [u8; 256],
) {
    if end <= start || end > 253 {
        return;
    }
    let mut psd = [0i32; 256];
    for bin in start..end {
        psd[bin] = 3072 - ((exps[bin] as i32) << 7);
    }
    // power per band
    let mut bndpsd = [0i32; 51];
    let mut j = start;
    let mut k = masktab[start] as usize;
    loop {
        let lastbin = BAND_START[k + 1].min(end);
        bndpsd[k] = psd[j];
        j += 1;
        while j < lastbin {
            bndpsd[k] = logadd(bndpsd[k], psd[j]);
            j += 1;
        }
        k += 1;
        if end <= lastbin {
            break;
        }
    }
    // excitation
    let bndstrt = masktab[start] as usize;
    let bndend = masktab[end - 1] as usize + 1;
    let mut excite = [0i32; 51];
    let (mut fastleak, mut slowleak);
    let begin;
    match cpl_leak {
        None => {
            let mut lowcomp = calc_lowcomp(0, bndpsd[0], bndpsd[1], 0);
            excite[0] = bndpsd[0] - fgain - lowcomp;
            lowcomp = calc_lowcomp(lowcomp, bndpsd[1], bndpsd[2], 1);
            excite[1] = bndpsd[1] - fgain - lowcomp;
            let mut b0 = 7;
            fastleak = 0;
            slowleak = 0;
            for bin in 2..7 {
                if bndend != 7 || bin != 6 {
                    lowcomp = calc_lowcomp(lowcomp, bndpsd[bin], bndpsd[bin + 1], bin);
                }
                fastleak = bndpsd[bin] - fgain;
                slowleak = bndpsd[bin] - p.sgain;
                excite[bin] = fastleak - lowcomp;
                if (bndend != 7 || bin != 6) && bndpsd[bin] <= bndpsd[bin + 1] {
                    b0 = bin + 1;
                    break;
                }
            }
            for bin in b0..bndend.min(22) {
                if bndend != 7 || bin != 6 {
                    lowcomp = calc_lowcomp(lowcomp, bndpsd[bin], bndpsd[bin + 1], bin);
                }
                fastleak = (fastleak - p.fdecay).max(bndpsd[bin] - fgain);
                slowleak = (slowleak - p.sdecay).max(bndpsd[bin] - p.sgain);
                excite[bin] = (fastleak - lowcomp).max(slowleak);
            }
            begin = 22;
        }
        Some((f, s)) => {
            begin = bndstrt;
            fastleak = f;
            slowleak = s;
        }
    }
    for bin in begin..bndend {
        fastleak = (fastleak - p.fdecay).max(bndpsd[bin] - fgain);
        slowleak = (slowleak - p.sdecay).max(bndpsd[bin] - p.sgain);
        excite[bin] = fastleak.max(slowleak);
    }
    // masking curve
    let mut mask = [0i32; 51];
    for bin in bndstrt..bndend {
        if bndpsd[bin] < p.dbknee {
            excite[bin] += (p.dbknee - bndpsd[bin]) >> 2;
        }
        mask[bin] = excite[bin].max(HTH[bin][p.fscod] as i32);
    }
    // delta bit allocation
    let mut band = bndstrt;
    for &(off, len, d) in delta {
        band += off;
        for _ in 0..len {
            if band < 50 {
                mask[band] += d;
            }
            band += 1;
        }
    }
    // pointers
    let mut i = start;
    let mut j = masktab[start] as usize;
    loop {
        let lastbin = BAND_START[j + 1].min(end);
        let mut m = mask[j] - snroffset - p.floor;
        if m < 0 {
            m = 0;
        }
        m &= 0x1fe0;
        m += p.floor;
        while i < lastbin {
            let addr = ((psd[i] - m) >> 5).clamp(0, 63) as usize;
            bap[i] = BAPTAB[addr];
            i += 1;
        }
        j += 1;
        if end <= lastbin {
            break;
        }
    }
}

/// Length in seconds of a plain .ac3 file: frames are all the same size, so it's the file size
/// over the first frame's size, times 1536 samples per frame.
pub fn raw_file_duration(path: &std::path::Path) -> Option<f64> {
    use std::io::Read;
    let size = std::fs::metadata(path).ok()?.len();
    let mut head = vec![0u8; 65536];
    let n = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    let head = &head[..n];
    let sync = head.windows(2).position(|w| w == [0x0B, 0x77])?;
    let h = head.get(sync..sync + 6)?;
    let fscod = (h[4] >> 6) as usize;
    let bytes = frame_bytes(fscod, (h[4] & 0x3f) as usize)?;
    let bsid = head.get(sync + 5).map(|b| b >> 3).unwrap_or(8);
    if bsid > 10 {
        return None;
    }
    let mut rate = SAMPLE_RATES[fscod] as f64;
    if bsid == 9 {
        rate /= 2.0;
    } else if bsid == 10 {
        rate /= 4.0;
    }
    let frames = (size - sync as u64) / bytes as u64;
    Some(frames as f64 * FRAME_SAMPLES as f64 / rate)
}

/// The two bytes after a frame confirm it: a new sync word, or zero padding.
pub fn next_frame_follows(a: u8, b: u8) -> bool {
    (a == SYNC_WORD[0] && b == SYNC_WORD[1]) || (a == 0 && b == 0)
}

const SYNC_WORD: [u8; 2] = [0x0B, 0x77];

/// Finds AC-3 frames in a byte stream (container packets may split or join them) and decodes them.
pub struct Ac3Stream {
    buf: Vec<u8>,
    dec: Ac3Decoder,
    next_pts: Option<f64>,
}

impl Default for Ac3Stream {
    fn default() -> Self {
        Self { buf: Vec::new(), dec: Ac3Decoder::new(), next_pts: None }
    }
}

impl Ac3Stream {
    pub fn reset(&mut self) {
        self.buf.clear();
        self.next_pts = None;
        self.dec.reset();
    }

    /// Add bytes that start at media time `pts`; returns the frames now complete with their times.
    pub fn push(&mut self, data: &[u8], pts: f64) -> Vec<(f64, Frame)> {
        self.process(data, pts, false)
    }

    /// End of stream: the last frame has no frame after it to confirm its sync word, so it is
    /// decoded now without that check.
    pub fn flush(&mut self) -> Vec<(f64, Frame)> {
        let pts = self.next_pts.unwrap_or(0.0);
        self.process(&[], pts, true)
    }

    fn process(&mut self, data: &[u8], pts: f64, last: bool) -> Vec<(f64, Frame)> {
        if self.buf.is_empty() || self.next_pts.is_none() {
            self.next_pts = Some(pts);
        }
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        loop {
            let Some(sync) = self.buf.windows(2).position(|w| w == [0x0B, 0x77]) else {
                let keep = self.buf.last().copied() == Some(0x0B);
                self.buf.clear();
                if keep {
                    self.buf.push(0x0B);
                }
                break;
            };
            if sync > 0 {
                self.buf.drain(..sync);
            }
            if self.buf.len() < 6 {
                break;
            }
            let fscod = (self.buf[4] >> 6) as usize;
            let frmsizecod = (self.buf[4] & 0x3f) as usize;
            let Some(size) = frame_bytes(fscod, frmsizecod) else {
                self.buf.drain(..1);
                continue;
            };
            if self.buf.len() < size {
                break;
            }
            if !last {
                // the original's packetizer only trusts a sync word when the bytes right after
                // the frame start another frame (or are padding zeros); otherwise it was a false
                // ("emulated") sync word inside other data (libpacketizer_a52 0x140002e3f..0x140002e6d)
                if self.buf.len() < size + 2 {
                    break;
                }
                if !next_frame_follows(self.buf[size], self.buf[size + 1]) {
                    self.buf.drain(..1);
                    continue;
                }
            }
            let pts = self.next_pts.unwrap_or(pts);
            match self.dec.decode_frame(&self.buf[..size]) {
                Some(f) => {
                    self.next_pts = Some(pts + FRAME_SAMPLES as f64 / f.rate as f64);
                    out.push((pts, f));
                    self.buf.drain(..size);
                }
                None => {
                    // not a frame we can read: look for the next sync word
                    self.buf.drain(..1);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
