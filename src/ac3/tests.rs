//! Tests for the AC-3 decoder: its tables against the copies found in the original's codec
//! library, and round trips through a small test-only encoder that writes standard AC-3 frames.

use super::*;

#[test]
fn frame_sizes_match_the_original_table() {
    // words per frame (48 / 44.1 / 32 kHz) as found at libavcodec_plugin.dll 0x140e7e6e0
    let table: [[usize; 3]; 38] = [
        [64, 69, 96], [64, 70, 96], [80, 87, 120], [80, 88, 120], [96, 104, 144], [96, 105, 144], [112, 121, 168],
        [112, 122, 168], [128, 139, 192], [128, 140, 192], [160, 174, 240], [160, 175, 240], [192, 208, 288],
        [192, 209, 288], [224, 243, 336], [224, 244, 336], [256, 278, 384], [256, 279, 384], [320, 348, 480],
        [320, 349, 480], [384, 417, 576], [384, 418, 576], [448, 487, 672], [448, 488, 672], [512, 557, 768],
        [512, 558, 768], [640, 696, 960], [640, 697, 960], [768, 835, 1152], [768, 836, 1152], [896, 975, 1344],
        [896, 976, 1344], [1024, 1114, 1536], [1024, 1115, 1536], [1152, 1253, 1728], [1152, 1254, 1728],
        [1280, 1393, 1920], [1280, 1394, 1920],
    ];
    for (code, row) in table.iter().enumerate() {
        for fs in 0..3 {
            assert_eq!(frame_bytes(fs, code), Some(row[fs] * 2), "code {code} fs {fs}");
        }
    }
}

#[test]
fn bands_match_the_original_table() {
    // bin → band as found at libavcodec_plugin.dll 0x14112e960
    let m = masktab();
    assert_eq!(&m[0..28], &(0u8..28).collect::<Vec<_>>()[..]);
    assert_eq!(&m[28..34], &[28, 28, 28, 29, 29, 29]);
    assert_eq!(m[49], 35);
    assert_eq!(m[55], 36);
    assert_eq!(m[252], 49);
}

#[test]
fn window_is_power_complementary() {
    let w = kbd_window();
    for n in 0..256 {
        let s = w[n] * w[n] + w[n + 256] * w[n + 256];
        assert!((s - 1.0).abs() < 1e-5, "n {n}: {s}");
    }
}

// ----- a small encoder, for tests only ----------------------------------------------------------

struct W {
    bytes: Vec<u8>,
    pos: usize,
}

impl W {
    fn bit(&mut self, p: usize, b: u32) {
        let i = p >> 3;
        if i >= self.bytes.len() {
            self.bytes.resize(i + 1, 0);
        }
        if b & 1 == 1 {
            self.bytes[i] |= 0x80 >> (p & 7);
        } else {
            self.bytes[i] &= !(0x80 >> (p & 7));
        }
    }
    fn put(&mut self, n: u32, v: u32) {
        for i in (0..n).rev() {
            self.bit(self.pos, v >> i);
            self.pos += 1;
        }
    }
    fn put_at(&mut self, at: usize, n: u32, v: u32) {
        for i in 0..n {
            self.bit(at + i as usize, v >> (n - 1 - i));
        }
    }
}

#[derive(Default)]
struct GW {
    p1: Option<(usize, Vec<i32>)>,
    p2: Option<(usize, Vec<i32>)>,
    p4: Option<(usize, Vec<i32>)>,
}

fn qsym(m: f64, levels: i32) -> i32 {
    (((m * levels as f64) + (levels - 1) as f64) / 2.0).round().clamp(0.0, (levels - 1) as f64) as i32
}

fn put_group(w: &mut W, slot: &mut Option<(usize, Vec<i32>)>, bits: u32, size: usize, code: fn(&[i32]) -> u32, q: i32) {
    match slot {
        None => {
            let at = w.pos;
            w.put(bits, 0);
            *slot = Some((at, vec![q]));
        }
        Some((_, v)) => v.push(q),
    }
    if let Some((at, v)) = slot {
        if v.len() == size {
            w.put_at(*at, bits, code(v));
            *slot = None;
        }
    }
}

fn flush_groups(w: &mut W, g: &mut GW) {
    let fill = |w: &mut W, slot: &mut Option<(usize, Vec<i32>)>, bits: u32, size: usize, mid: i32, code: fn(&[i32]) -> u32| {
        if let Some((at, mut v)) = slot.take() {
            while v.len() < size {
                v.push(mid);
            }
            w.put_at(at, bits, code(&v));
        }
    };
    fill(w, &mut g.p1, 5, 3, 1, |v| (v[0] * 9 + v[1] * 3 + v[2]) as u32);
    fill(w, &mut g.p2, 7, 3, 2, |v| (v[0] * 25 + v[1] * 5 + v[2]) as u32);
    fill(w, &mut g.p4, 7, 2, 5, |v| (v[0] * 11 + v[1]) as u32);
}

fn put_mant(w: &mut W, g: &mut GW, bap: u8, m: f64) {
    match bap {
        0 => {}
        1 => put_group(w, &mut g.p1, 5, 3, |v| (v[0] * 9 + v[1] * 3 + v[2]) as u32, qsym(m, 3)),
        2 => put_group(w, &mut g.p2, 7, 3, |v| (v[0] * 25 + v[1] * 5 + v[2]) as u32, qsym(m, 5)),
        3 => w.put(3, qsym(m, 7) as u32),
        4 => put_group(w, &mut g.p4, 7, 2, |v| (v[0] * 11 + v[1]) as u32, qsym(m, 11)),
        5 => w.put(4, qsym(m, 15) as u32),
        _ => {
            let bits = QBITS[bap as usize];
            let half = (1i64 << (bits - 1)) as f64;
            let q = (m * half).round().clamp(-half, half - 1.0) as i64;
            w.put(bits, (q as u32) & ((1u32 << bits) - 1));
        }
    }
}

/// Forward transform of a windowed 512-sample block, as the standard defines it.
fn forward(xw: &[f64; 512], short: bool) -> [f64; 256] {
    let mut out = [0f64; 256];
    if !short {
        for k in 0..256 {
            let mut s = 0.0;
            for n in 0..512 {
                s += xw[n] * kernel(512, 0.0, n, k);
            }
            out[k] = -2.0 / 512.0 * s;
        }
    } else {
        for k in 0..128 {
            let (mut s1, mut s2) = (0.0, 0.0);
            for n in 0..256 {
                s1 += xw[n] * kernel(256, -1.0, n, k);
                s2 += xw[256 + n] * kernel(256, 1.0, n, k);
            }
            out[2 * k] = -2.0 / 256.0 * s1;
            out[2 * k + 1] = -2.0 / 256.0 * s2;
        }
    }
    out
}

fn exponents(c: &[f64], n: usize, first_max: u8) -> Vec<u8> {
    let mut e: Vec<u8> = c[..n]
        .iter()
        .map(|v| if v.abs() < 1e-12 { 24 } else { ((-v.abs().log2()).floor()).clamp(0.0, 24.0) as u8 })
        .collect();
    e[0] = e[0].min(first_max);
    for i in 1..n {
        e[i] = e[i].min(e[i - 1] + 2);
    }
    for i in (1..n).rev() {
        e[i - 1] = e[i - 1].min(e[i] + 2);
    }
    e[0] = e[0].min(first_max);
    for i in 1..n {
        e[i] = e[i].min(e[i - 1] + 2);
    }
    e
}

fn put_exps(w: &mut W, e: &[u8], ngrps: usize) {
    w.put(4, e[0] as u32);
    let mut prev = e[0] as i32;
    for g in 0..ngrps {
        let mut d = [2i32; 3];
        for (j, dd) in d.iter_mut().enumerate() {
            let i = 1 + g * 3 + j;
            let target = if i < e.len() { e[i] as i32 } else { prev };
            *dd = (target - prev).clamp(-2, 2) + 2;
            prev += *dd - 2;
        }
        w.put(7, (d[0] * 25 + d[1] * 5 + d[2]) as u32);
    }
}

/// Encode `input` (per channel, coded order; LFE last if `lfe`) into 48 kHz, 640 kb/s frames.
fn encode(input: &[Vec<f64>], acmod: usize, lfe: bool, short_block: Option<usize>) -> Vec<u8> {
    let nf = NFCHANS[acmod];
    let nch = nf + lfe as usize;
    let frames = input[0].len() / FRAME_SAMPLES;
    let win = kbd_window();
    let mt = masktab();
    let mut out = Vec::new();
    let mut prev = vec![[0f64; 256]; nch];
    for f in 0..frames {
        // transform all six blocks first; the quality is then lowered until the frame fits
        let mut blocks = Vec::new();
        for blk in 0..6 {
            let start = (f * 6 + blk) * 256;
            let mut coeffs = vec![[0f64; 256]; nch];
            let mut short = vec![false; nch];
            for ch in 0..nch {
                let mut xw = [0f64; 512];
                for n in 0..256 {
                    xw[n] = prev[ch][n] * win[n] as f64;
                    xw[256 + n] = input[ch][start + n] * win[256 + n] as f64;
                }
                for n in 0..256 {
                    prev[ch][n] = input[ch][start + n];
                }
                short[ch] = ch < nf && short_block == Some(blk);
                coeffs[ch] = forward(&xw, short[ch]);
            }
            blocks.push((coeffs, short));
        }
        let mut csnr = 40u32;
        let frame = loop {
            let w = write_frame(&blocks, acmod, lfe, nf, nch, csnr, &mt);
            if w.pos <= 2560 * 8 || csnr == 0 {
                break w;
            }
            csnr -= 1;
        };
        let mut w = frame;
        assert!(w.pos <= 2560 * 8, "frame too big: {} bits", w.pos);
        w.bytes.resize(2560, 0);
        out.extend_from_slice(&w.bytes);
    }
    out
}

#[allow(clippy::type_complexity)]
fn write_frame(blocks: &[(Vec<[f64; 256]>, Vec<bool>)], acmod: usize, lfe: bool, nf: usize, nch: usize, csnr: u32, mt: &[u8; 256]) -> W {
    {
        let mut w = W { bytes: vec![0; 2560], pos: 0 };
        w.put(16, 0x0B77);
        w.put(16, 0); // crc1 (not checked)
        w.put(2, 0); // 48 kHz
        w.put(6, 36); // 640 kb/s
        w.put(5, 8); // bsid
        w.put(3, 0); // bsmod
        w.put(3, acmod as u32);
        if acmod & 1 != 0 && acmod != 1 {
            w.put(2, 0);
        }
        if acmod & 4 != 0 {
            w.put(2, 0);
        }
        if acmod == 2 {
            w.put(2, 0);
        }
        w.put(1, lfe as u32);
        w.put(5, 31); // dialnorm
        w.put(3, 0); // compre, langcode, audprodie
        w.put(2, 1); // copyright, original
        w.put(3, 0); // timecodes, addbsi
        for (blk, (coeffs, short)) in blocks.iter().enumerate() {
            for &s in short.iter().take(nf) {
                w.put(1, s as u32);
            }
            for _ in 0..nf {
                w.put(1, 0); // dither off
            }
            w.put(1, 0); // dynrng
            if acmod == 0 {
                w.put(1, 0);
            }
            if blk == 0 {
                w.put(1, 1); // cplstre
                w.put(1, 0); // no coupling
            } else {
                w.put(1, 0);
            }
            if acmod == 2 {
                if blk == 0 {
                    w.put(1, 1);
                    w.put(4, 0);
                } else {
                    w.put(1, 0);
                }
            }
            for _ in 0..nf {
                w.put(2, 1); // D15
            }
            if lfe {
                w.put(1, 1);
            }
            for _ in 0..nf {
                w.put(6, 60); // full bandwidth
            }
            let endmant = 253;
            let mut exps = vec![[0u8; 256]; nch];
            for ch in 0..nf {
                let e = exponents(&coeffs[ch], endmant, 15);
                exps[ch][..endmant].copy_from_slice(&e);
                put_exps(&mut w, &e, (endmant - 1) / 3);
                w.put(2, 0); // gainrng
            }
            if lfe {
                let e = exponents(&coeffs[nf], 7, 15);
                exps[nf][..7].copy_from_slice(&e);
                put_exps(&mut w, &e, 2);
            }
            if blk == 0 {
                w.put(1, 1);
                w.put(2, 2);
                w.put(2, 1);
                w.put(2, 1);
                w.put(2, 2);
                w.put(3, 7);
                w.put(1, 1);
                w.put(6, csnr);
                for _ in 0..nch {
                    w.put(4, 0);
                    w.put(3, 4);
                }
            } else {
                w.put(1, 0);
                w.put(1, 0);
            }
            w.put(1, 0); // deltbaie
            w.put(1, 0); // skiple
            let p = BaParams { sdecay: SLOW_DECAY[2], fdecay: FAST_DECAY[1], sgain: SLOW_GAIN[1], dbknee: DB_PER_BIT[2], floor: FLOOR[7], fscod: 0 };
            let snr = (((csnr as i32 - 15) << 4) + 0) << 2;
            let mut g = GW::default();
            for ch in 0..nch {
                let end = if ch < nf { endmant } else { 7 };
                let mut bap = [0u8; 256];
                bit_allocation(mt, &exps[ch], 0, end, &p, FAST_GAIN[4], snr, None, &[], &mut bap);
                for bin in 0..end {
                    let m = coeffs[ch][bin] * (1u64 << exps[ch][bin]) as f64;
                    put_mant(&mut w, &mut g, bap[bin], m.clamp(-0.999_999, 0.999_999));
                }
            }
            flush_groups(&mut w, &mut g);
        }
        w
    }
}

fn tone(freq: f64, amp: f64, len: usize) -> Vec<f64> {
    (0..len).map(|i| amp * (2.0 * PI * freq * i as f64 / 48000.0).sin()).collect()
}

fn decode_all(stream: &[u8]) -> (usize, Vec<f32>) {
    let mut s = Ac3Stream::default();
    let mut pcm = Vec::new();
    let mut ch = 0;
    let mut frames = Vec::new();
    for chunk in stream.chunks(1000) {
        frames.extend(s.push(chunk, 0.0));
    }
    frames.extend(s.flush());
    for (_, f) in frames {
        ch = f.channels;
        assert_eq!(f.rate, 48000);
        pcm.extend_from_slice(&f.samples);
    }
    (ch, pcm)
}

#[test]
fn false_sync_words_are_skipped_like_the_original() {
    let len = FRAME_SAMPLES * 3;
    let left = tone(440.0, 0.4, len);
    let stream = encode(&[left.clone(), left.clone()], 2, false, None);
    // junk in front that contains a sync word and a believable header, but no frame after it
    let size = frame_bytes((stream[4] >> 6) as usize, (stream[4] & 0x3f) as usize).unwrap();
    let mut junk = vec![0x55u8; size + 100];
    junk[10] = 0x0B;
    junk[11] = 0x77;
    junk[14] = stream[4];
    junk[15] = stream[5];
    let mut with_junk = junk;
    with_junk.extend_from_slice(&stream);
    let (_, clean) = decode_all(&stream);
    let (_, got) = decode_all(&with_junk);
    assert_eq!(got.len(), clean.len());
    assert_eq!(got, clean);
    assert!(next_frame_follows(0x0B, 0x77));
    assert!(next_frame_follows(0, 0));
    assert!(!next_frame_follows(0x0B, 0x00));
}

fn snr_db(reference: &[f64], got: &[f32], ch: usize, slot: usize, delay: usize) -> f64 {
    let (mut sig, mut err) = (0.0, 0.0);
    let n = got.len() / ch;
    for t in 2048..n {
        let r = reference[t - delay];
        let g = got[t * ch + slot] as f64;
        sig += r * r;
        err += (r - g) * (r - g);
    }
    10.0 * (sig / err.max(1e-30)).log10()
}

#[test]
fn stereo_round_trip_long_blocks() {
    let len = FRAME_SAMPLES * 6;
    let left = tone(440.0, 0.4, len);
    let right: Vec<f64> = tone(1000.0, 0.3, len).iter().zip(tone(3500.0, 0.1, len)).map(|(a, b)| a + b).collect();
    let stream = encode(&[left.clone(), right.clone()], 2, false, None);
    let (ch, pcm) = decode_all(&stream);
    assert_eq!(ch, 2);
    assert_eq!(pcm.len(), len * 2);
    let l = snr_db(&left, &pcm, 2, 0, 256);
    let r = snr_db(&right, &pcm, 2, 1, 256);
    assert!(l > 40.0 && r > 40.0, "SNR left {l:.1} dB, right {r:.1} dB");
}

#[test]
fn stereo_round_trip_with_short_blocks() {
    let len = FRAME_SAMPLES * 6;
    let left = tone(700.0, 0.5, len);
    let right = tone(2200.0, 0.3, len);
    let stream = encode(&[left.clone(), right.clone()], 2, false, Some(3));
    let (_, pcm) = decode_all(&stream);
    let l = snr_db(&left, &pcm, 2, 0, 256);
    let r = snr_db(&right, &pcm, 2, 1, 256);
    assert!(l > 40.0 && r > 40.0, "SNR left {l:.1} dB, right {r:.1} dB");
}

/// Writes a 6-second 5.1 test file for trying the player: `cargo test write_sample_ac3 -- --ignored`
#[test]
#[ignore]
fn write_sample_ac3() {
    let len = FRAME_SAMPLES * 188; // about 6 s
    let freqs = [440.0, 550.0, 660.0, 330.0, 220.0, 55.0];
    let input: Vec<Vec<f64>> = freqs.iter().map(|&f| tone(f, 0.2, len)).collect();
    let stream = encode(&input, 7, true, None);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shots");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tone51.ac3"), stream).unwrap();
}

#[test]
fn surround_channels_land_in_windows_order() {
    // coded order for 3/2 + LFE: L, C, R, SL, SR, then LFE
    let len = FRAME_SAMPLES * 4;
    let freqs = [300.0, 500.0, 700.0, 900.0, 1100.0, 60.0];
    let input: Vec<Vec<f64>> = freqs.iter().map(|&f| tone(f, 0.25, len)).collect();
    let stream = encode(&input, 7, true, None);
    let (ch, pcm) = decode_all(&stream);
    assert_eq!(ch, 6);
    // Windows order: FL FR C LFE SL SR
    let slot_of = [0usize, 2, 1, 4, 5, 3];
    for (coded, &slot) in slot_of.iter().enumerate() {
        let s = snr_db(&input[coded], &pcm, 6, slot, 256);
        assert!(s > 25.0, "channel {coded} → slot {slot}: {s:.1} dB");
    }
}
