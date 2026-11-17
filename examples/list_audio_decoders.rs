//! Lists the Windows audio decoders that accept a given input (default: Dolby AC-3 and E-AC-3).
//! Run: cargo run --example list_audio_decoders

#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
        for (name, sub) in [("AC-3", MFAudioFormat_Dolby_AC3), ("E-AC-3", MFAudioFormat_Dolby_DDPlus), ("AAC", MFAudioFormat_AAC)] {
            let input = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Audio, guidSubtype: sub };
            let mut acts: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count = 0u32;
            let flags = MFT_ENUM_FLAG_ALL;
            MFTEnumEx(MFT_CATEGORY_AUDIO_DECODER, flags, Some(&input), None, &mut acts, &mut count)?;
            println!("{name}: {count} decoder(s)");
            for i in 0..count as usize {
                if let Some(a) = (*acts.add(i)).take() {
                    let mut p = windows::core::PWSTR::null();
                    let mut len = 0u32;
                    if a.GetAllocatedString(&MFT_FRIENDLY_NAME_Attribute, &mut p, &mut len).is_ok() {
                        println!("  - {}", p.to_string().unwrap_or_default());
                        CoTaskMemFree(Some(p.0 as _));
                    }
                }
            }
            if !acts.is_null() {
                CoTaskMemFree(Some(acts as _));
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
