//! Simple SubRip (.srt) subtitles.

use std::path::Path;

pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

pub struct Subtitles {
    pub name: String,
    pub cues: Vec<Cue>,
    pub delay: f64,
}

impl Subtitles {
    pub fn load(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        let text = decode(&bytes);
        let cues = parse_srt(&text);
        if cues.is_empty() {
            return None;
        }
        Some(Self {
            name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            cues,
            delay: 0.0,
        })
    }

    /// Look for "movie.srt" (or "movie.en.srt") beside "movie.mp4".
    pub fn find_beside(location: &str) -> Option<Self> {
        if location.contains("://") {
            return None;
        }
        let p = Path::new(location);
        let stem = p.file_stem()?.to_string_lossy().to_string();
        let dir = p.parent()?;
        let direct = dir.join(format!("{stem}.srt"));
        if direct.is_file() {
            return Self::load(&direct);
        }
        let rd = std::fs::read_dir(dir).ok()?;
        for e in rd.flatten() {
            let path = e.path();
            let name = path.file_name()?.to_string_lossy().to_string();
            if name.starts_with(&format!("{stem}.")) && name.to_lowercase().ends_with(".srt") {
                return Self::load(&path);
            }
        }
        None
    }

    pub fn at(&self, t: f64) -> Option<&str> {
        let t = t - self.delay;
        // cues are sorted by start
        let idx = self.cues.partition_point(|c| c.start <= t);
        for c in self.cues[..idx].iter().rev().take(4) {
            if t >= c.start && t < c.end {
                return Some(&c.text);
            }
        }
        None
    }
}

fn decode(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let u: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&u);
    }
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        // fall back to Latin-1 for older files
        Err(_) => b.iter().map(|&c| c as char).collect(),
    }
}

fn parse_stamp(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: f64 = parts[0].trim().parse().ok()?;
    let m: f64 = parts[1].trim().parse().ok()?;
    let sec: f64 = parts[2].trim().parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            '{' => in_tag = true,
            '}' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

pub fn parse_srt(text: &str) -> Vec<Cue> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty() || false).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let mut it = lines[ti].split("-->");
        let (Some(a), Some(b)) = (it.next(), it.next()) else { continue };
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (parse_stamp(a), parse_stamp(b)) else { continue };
        let body: Vec<String> = lines[ti + 1..].iter().map(|l| strip_tags(l)).collect();
        if body.is_empty() {
            continue;
        }
        cues.push(Cue { start, end, text: body.join("\n") });
    }
    cues.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    cues
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt() {
        let s = "1\n00:00:01,000 --> 00:00:02,500\nHello <i>there</i>\n\n2\n00:00:03,000 --> 00:00:04,000\nSecond\nline\n";
        let c = parse_srt(s);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].text, "Hello there");
        let subs = Subtitles { name: String::new(), cues: c, delay: 0.0 };
        assert_eq!(subs.at(1.5), Some("Hello there"));
        assert_eq!(subs.at(2.7), None);
        assert_eq!(subs.at(3.5), Some("Second\nline"));
    }
}
