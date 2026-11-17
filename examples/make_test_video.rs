//! Makes a short test clip (moving colour bars, a white block, a tone with a beep every second)
//! so the player can be tried without anyone's own videos.
//! Run: cargo run --example make_test_video -- <out.mp4> [seconds]

#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "test.mp4".into());
    let seconds: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(8);
    let _ = std::fs::remove_file(&out);
    let (w, h, fps) = (640u32, 360u32, 30u32);
    let (rate, ch) = (44100u32, 2u32);

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
        let wout: Vec<u16> = out.encode_utf16().chain(std::iter::once(0)).collect();
        let writer = MFCreateSinkWriterFromURL(PCWSTR(wout.as_ptr()), None::<&IMFByteStream>, None::<&IMFAttributes>)?;

        // video: H.264 out, RGB32 in
        let vo = MFCreateMediaType()?;
        vo.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        vo.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        vo.SetUINT32(&MF_MT_AVG_BITRATE, 1_500_000)?;
        vo.SetUINT32(&MF_MT_INTERLACE_MODE, 2)?;
        vo.SetUINT64(&MF_MT_FRAME_SIZE, ((w as u64) << 32) | h as u64)?;
        vo.SetUINT64(&MF_MT_FRAME_RATE, ((fps as u64) << 32) | 1)?;
        vo.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
        let vs = writer.AddStream(&vo)?;
        let vi = MFCreateMediaType()?;
        vi.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        vi.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        vi.SetUINT32(&MF_MT_INTERLACE_MODE, 2)?;
        vi.SetUINT64(&MF_MT_FRAME_SIZE, ((w as u64) << 32) | h as u64)?;
        vi.SetUINT64(&MF_MT_FRAME_RATE, ((fps as u64) << 32) | 1)?;
        vi.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
        vi.SetUINT32(&MF_MT_DEFAULT_STRIDE, w * 4)?;
        writer.SetInputMediaType(vs, &vi, None::<&IMFAttributes>)?;

        // audio: AAC out, 16-bit PCM in
        let ao = MFCreateMediaType()?;
        ao.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        ao.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
        ao.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        ao.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
        ao.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, ch)?;
        ao.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 16000)?;
        let as_ = writer.AddStream(&ao)?;
        let ai = MFCreateMediaType()?;
        ai.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        ai.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        ai.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        ai.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
        ai.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, ch)?;
        ai.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, ch * 2)?;
        ai.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, rate * ch * 2)?;
        writer.SetInputMediaType(as_, &ai, None::<&IMFAttributes>)?;

        writer.BeginWriting()?;
        let frame_dur = 10_000_000i64 / fps as i64;
        let total_frames = seconds * fps;
        let mut audio_written = 0u64;
        for f in 0..total_frames {
            let t = f as f32 / fps as f32;
            // picture
            let mut px = vec![0u8; (w * h * 4) as usize];
            let bars: [[u8; 3]; 7] = [[192, 192, 192], [192, 192, 0], [0, 192, 192], [0, 192, 0], [192, 0, 192], [192, 0, 0], [0, 0, 192]];
            let shift = (t * 40.0) as u32;
            for y in 0..h {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    let bar = (((x + shift) % w) * 7 / w) as usize;
                    let mut c = bars[bar];
                    if y > h * 3 / 4 {
                        let g = (x * 255 / w) as u8;
                        c = [g, g, g];
                    }
                    // moving white block
                    let bx = ((t / seconds as f32) * (w - 60) as f32) as u32;
                    if x >= bx && x < bx + 60 && y >= h / 2 - 30 && y < h / 2 + 30 {
                        c = [255, 255, 255];
                    }
                    // second counter: a row of squares
                    let sec = t as u32;
                    if y >= 16 && y < 36 && x >= 16 && x < 16 + 28 * (sec + 1) && (x - 16) % 28 < 20 {
                        c = [255, 80, 0];
                    }
                    px[i] = c[2];
                    px[i + 1] = c[1];
                    px[i + 2] = c[0];
                    px[i + 3] = 255;
                }
            }
            write(&writer, vs, &px, f as i64 * frame_dur, frame_dur)?;

            // sound up to the end of this frame
            let until = ((f + 1) as u64 * rate as u64) / fps as u64;
            let n = (until - audio_written) as usize;
            let mut pcm = Vec::with_capacity(n * 4);
            for k in 0..n {
                let s = (audio_written + k as u64) as f32 / rate as f32;
                let beep = s.fract() < 0.1;
                let freq = if beep { 880.0 } else { 440.0 };
                let amp = if beep { 0.35 } else { 0.12 };
                let v = ((s * freq * std::f32::consts::TAU).sin() * amp * 32767.0) as i16;
                pcm.extend_from_slice(&v.to_le_bytes());
                pcm.extend_from_slice(&v.to_le_bytes());
            }
            let start = audio_written as i64 * 10_000_000 / rate as i64;
            let dur = n as i64 * 10_000_000 / rate as i64;
            write(&writer, as_, &pcm, start, dur)?;
            audio_written = until;
        }
        writer.Finalize()?;
        let _ = MFShutdown();
    }
    println!("wrote {out}");
    Ok(())
}

#[cfg(windows)]
unsafe fn write(
    writer: &windows::Win32::Media::MediaFoundation::IMFSinkWriter,
    stream: u32,
    data: &[u8],
    time: i64,
    dur: i64,
) -> windows::core::Result<()> {
    use windows::Win32::Media::MediaFoundation::*;
    let buf = MFCreateMemoryBuffer(data.len() as u32)?;
    let mut p: *mut u8 = std::ptr::null_mut();
    buf.Lock(&mut p, None, None)?;
    std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
    buf.Unlock()?;
    buf.SetCurrentLength(data.len() as u32)?;
    let s = MFCreateSample()?;
    s.AddBuffer(&buf)?;
    s.SetSampleTime(time)?;
    s.SetSampleDuration(dur)?;
    writer.WriteSample(stream, &s)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only");
}
