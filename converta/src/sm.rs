use std::collections::HashMap;

pub type Tags = HashMap<String, String>;
const EPS: f64 = 1e-6;

#[derive(Debug, Clone, Default)]
pub struct RawChart {
    pub steps_type: String,
    pub description: String,
    pub difficulty: String,
    pub meter: String,
    pub data: String,
    pub local: Tags,
}

#[derive(Debug, Clone, Default)]
pub struct Song {
    pub tags: Tags,
    pub charts: Vec<RawChart>,
}

impl Song {
    pub fn tag(&self, key: &str) -> &str {
        self.tags.get(key).map(|s| s.trim()).unwrap_or("")
    }
}

pub fn decode_text(bytes: &[u8]) -> String {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&c| c as char).collect(),
    }
}

struct Tag {
    key: String,
    params: Vec<String>,
}

fn find_comment(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn at_line_start(chars: &[char], mut j: usize) -> bool {
    while j > 0 {
        let p = chars[j - 1];
        if p == '\n' {
            return true;
        }
        if !p.is_whitespace() {
            return false;
        }
        j -= 1;
    }
    true
}

fn tokenize(text: &str) -> Vec<Tag> {
    let mut cleaned = String::with_capacity(text.len());
    for line in text.lines() {
        let line = match find_comment(line) {
            Some(i) => &line[..i],
            None => line,
        };
        cleaned.push_str(line);
        cleaned.push('\n');
    }
    let chars: Vec<char> = cleaned.chars().collect();
    let n = chars.len();
    let mut tags = Vec::new();
    let mut i = 0;
    while i < n {
        if chars[i] != '#' {
            i += 1;
            continue;
        }
        i += 1;
        let mut key = String::new();
        while i < n && chars[i] != ':' && chars[i] != ';' {
            key.push(chars[i]);
            i += 1;
        }
        let mut params = Vec::new();
        let mut cur = String::new();
        if i < n && chars[i] == ':' {
            i += 1;
        }
        loop {
            if i >= n {
                params.push(cur.clone());
                break;
            }
            let c = chars[i];
            match c {
                '\\' => {
                    if i + 1 < n {
                        cur.push(chars[i + 1]);
                    }
                    i += 2;
                }
                ':' => {
                    params.push(cur.clone());
                    cur.clear();
                    i += 1;
                }
                ';' => {
                    params.push(cur.clone());
                    i += 1;
                    break;
                }
                '#' if at_line_start(&chars, i) => {
                    params.push(cur.clone());
                    break;
                }
                _ => {
                    cur.push(c);
                    i += 1;
                }
            }
        }
        tags.push(Tag { key: key.trim().to_uppercase(), params });
    }
    tags
}

pub fn parse_sm(text: &str) -> Song {
    let mut song = Song::default();
    for t in tokenize(text) {
        if (t.key == "NOTES" || t.key == "NOTES2") && t.params.len() >= 6 {
            song.charts.push(RawChart {
                steps_type: t.params[0].trim().to_string(),
                description: t.params[1].trim().to_string(),
                difficulty: t.params[2].trim().to_string(),
                meter: t.params[3].trim().to_string(),
                data: t.params[5..].join(":"),
                local: Tags::new(),
            });
        } else {
            song.tags.insert(t.key, t.params.join(":"));
        }
    }
    song
}

pub fn parse_ssc(text: &str) -> Song {
    let mut song = Song::default();
    let mut current: Option<RawChart> = None;
    for t in tokenize(text) {
        let value = t.params.join(":");
        match t.key.as_str() {
            "NOTEDATA" => current = Some(RawChart::default()),
            "NOTES" | "NOTES2" if current.is_some() => {
                let mut c = current.take().unwrap();
                c.data = value;
                song.charts.push(c);
            }
            key => {
                if let Some(c) = current.as_mut() {
                    match key {
                        "STEPSTYPE" => c.steps_type = value.trim().to_string(),
                        "DESCRIPTION" => c.description = value.trim().to_string(),
                        "DIFFICULTY" => c.difficulty = value.trim().to_string(),
                        "METER" => c.meter = value.trim().to_string(),
                        _ => {
                            c.local.insert(t.key, value);
                        }
                    }
                } else {
                    song.tags.insert(t.key, value);
                }
            }
        }
    }
    song
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteKind { Tap, HoldHead, HoldTail }

#[derive(Debug, Clone, Copy)]
pub struct NoteEvent {
    pub beat: f64,
    pub col: usize,
    pub kind: NoteKind,
}

pub fn parse_notes(data: &str) -> (usize, Vec<NoteEvent>) {
    let mut cols = 0usize;
    let mut events = Vec::new();
    for (m, measure) in data.split(',').enumerate() {
        let lines: Vec<Vec<char>> = measure
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| l.chars().collect())
            .collect();
        let n = lines.len();
        for (i, line) in lines.iter().enumerate() {
            if cols == 0 {
                cols = line.len();
            }
            if line.len() != cols {
                continue;
            }
            let beat = m as f64 * 4.0 + 4.0 * i as f64 / n as f64;
            for (col, ch) in line.iter().enumerate() {
                let kind = match ch {
                    '1' | 'L' | 'l' => NoteKind::Tap,
                    '2' | '4' => NoteKind::HoldHead,
                    '3' => NoteKind::HoldTail,
                    _ => continue,
                };
                events.push(NoteEvent { beat, col, kind });
            }
        }
    }
    (cols, events)
}

fn parse_pairs(s: &str) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = s
        .split(',')
        .filter_map(|p| {
            let (a, b) = p.split_once('=')?;
            let a: f64 = a.trim().parse().ok()?;
            let b: f64 = b.trim().parse().ok()?;
            (a.is_finite() && b.is_finite()).then_some((a, b))
        })
        .collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    v
}

#[derive(Debug, Clone)]
pub struct Timing {
    offset: f64,
    bpms: Vec<(f64, f64)>,
    stops: Vec<(f64, f64)>,
    delays: Vec<(f64, f64)>,
    warps: Vec<(f64, f64)>,
    fakes: Vec<(f64, f64)>,
}

impl Timing {
    pub fn new(tags: &Tags) -> Self {
        let get = |k: &str| tags.get(k).map(|s| s.as_str()).unwrap_or("");
        let mut bpms: Vec<_> = parse_pairs(get("BPMS")).into_iter().filter(|&(_, v)| v.abs() > 1e-9).collect();
        if bpms.is_empty() {
            bpms.push((0.0, 120.0));
        }
        bpms[0].0 = 0.0;
        Timing {
            offset: get("OFFSET").trim().parse().unwrap_or(0.0),
            bpms,
            stops: parse_pairs(get("STOPS")),
            delays: parse_pairs(get("DELAYS")),
            warps: parse_pairs(get("WARPS")).into_iter().filter(|&(_, l)| l > 0.0).collect(),
            fakes: parse_pairs(get("FAKES")).into_iter().filter(|&(_, l)| l > 0.0).collect(),
        }
    }

    fn bpm_secs(&self, beat: f64) -> f64 {
        let mut t = 0.0;
        let mut cur = 0.0;
        let mut bpm = self.bpms[0].1;
        for &(b, v) in &self.bpms[1..] {
            if b >= beat {
                break;
            }
            t += (b - cur) * 60.0 / bpm;
            cur = b;
            bpm = v;
        }
        t + (beat - cur) * 60.0 / bpm
    }

    pub fn bpm_at(&self, beat: f64) -> f64 {
        let mut bpm = self.bpms[0].1;
        for &(b, v) in &self.bpms {
            if b <= beat + EPS {
                bpm = v;
            }
        }
        bpm
    }

    pub fn time_at(&self, beat: f64) -> f64 {
        let mut t = self.bpm_secs(beat);
        for &(b, s) in &self.stops {
            if b < beat - EPS {
                t += s;
            }
        }
        for &(b, s) in &self.delays {
            if b <= beat + EPS {
                t += s;
            }
        }
        for &(w, l) in &self.warps {
            if beat > w + EPS {
                let end = beat.min(w + l);
                t -= self.bpm_secs(end) - self.bpm_secs(w);
            }
        }
        t - self.offset
    }

    pub fn is_unhittable(&self, beat: f64) -> bool {
        self.warps.iter().chain(self.fakes.iter()).any(|&(w, l)| beat >= w - EPS && beat < w + l - EPS)
    }

    pub fn timing_points(&self) -> Vec<(i64, f64)> {
        let mut v: Vec<(f64, f64)> = Vec::new();
        for &(b, bpm) in &self.bpms {
            if bpm > 0.0 {
                v.push((self.time_at(b) * 1000.0, bpm));
            }
        }
        for &(b, s) in &self.stops {
            if s > 0.0 {
                let bpm = self.bpm_at(b);
                if bpm > 0.0 {
                    v.push(((self.time_at(b) + s) * 1000.0, bpm));
                }
            }
        }
        for &(b, s) in &self.delays {
            let bpm = self.bpm_at(b);
            if s > 0.0 && bpm > 0.0 {
                v.push((self.time_at(b) * 1000.0, bpm));
            }
        }
        v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut out: Vec<(i64, f64)> = Vec::new();
        for (ms, bpm) in v {
            let r = ms.round() as i64;
            match out.last_mut() {
                Some(last) if last.0 == r => last.1 = bpm,
                _ => out.push((r, bpm)),
            }
        }
        if out.is_empty() {
            out.push((0, 120.0));
        }
        out
    }
}