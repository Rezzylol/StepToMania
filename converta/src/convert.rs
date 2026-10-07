use crate::sm::{self, NoteKind, Timing};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::Write as _;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[derive(Clone)]
pub struct Options {
    pub od: f32,
    pub hp: f32,
    pub out_dir: PathBuf,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Level { Info, Ok, Warn, Error }

pub enum Msg {
    Log(Level, String),
    Progress { done: usize, total: usize },
    Done { created: usize, failed: usize, out_dir: PathBuf },
}

pub struct Report {
    pub osz: PathBuf,
    pub charts: usize,
    pub warnings: Vec<String>,
}

struct Obj {
    col: usize,
    start: i64,
    end: Option<i64>,
}

fn ext_lower(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn is_chart_file(p: &Path) -> bool {
    matches!(ext_lower(p).as_str(), "sm" | "ssc")
}

fn walk(p: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if p.is_file() {
        if is_chart_file(p) {
            out.push(p.to_path_buf());
        }
        return;
    }
    if depth > 8 {
        return;
    }
    if let Ok(rd) = fs::read_dir(p) {
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                walk(&path, out, depth + 1);
            } else if is_chart_file(&path) {
                out.push(path);
            }
        }
    }
}

pub fn collect_charts(inputs: &[PathBuf]) -> Vec<PathBuf> {
    let mut all = Vec::new();
    for p in inputs {
        walk(p, &mut all, 0);
    }
    all.sort();
    all.dedup();
    all.retain(|p| ext_lower(p) != "ssc" || !(p.with_extension("sm").exists() || p.with_extension("SM").exists()));
    all
}

pub fn default_output_dir(inputs: &[PathBuf]) -> PathBuf {
    // Dropped folder -> next to it; dropped file -> in its folder.
    let base = inputs.first().and_then(|p| p.parent().map(|x| x.to_path_buf()));
    base.or_else(|| std::env::current_dir().ok()).unwrap_or_else(|| PathBuf::from(".")).join("osz output")
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let p = dir.join(name);
    if p.is_file() {
        return Some(p);
    }
    let want = Path::new(name).file_name()?.to_string_lossy().to_lowercase();
    for e in fs::read_dir(p.parent()?).ok()?.flatten() {
        if e.file_name().to_string_lossy().to_lowercase() == want && e.path().is_file() {
            return Some(e.path());
        }
    }
    None
}

fn files_with_ext(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && exts.contains(&ext_lower(p).as_str())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn guess_audio(dir: &Path) -> Option<PathBuf> {
    files_with_ext(dir, &["ogg", "mp3", "wav"]).into_iter().max_by_key(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

fn is_image(p: &Path) -> bool {
    matches!(ext_lower(p).as_str(), "jpg" | "jpeg" | "png")
}

fn find_background(song: &sm::Song, dir: &Path) -> Option<PathBuf> {
    for key in ["BACKGROUND", "BANNER"] {
        if let Some(p) = find_file(dir, song.tag(key)) {
            if is_image(&p) {
                return Some(p);
            }
        }
    }
    let imgs = files_with_ext(dir, &["jpg", "jpeg", "png"]);
    let lower = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    imgs.iter().find(|p| { let n = lower(p); n.contains("bg") || n.contains("background") }).cloned()
}

fn clean(s: &str) -> String {
    s.replace(['\r', '\n'], " ").trim().to_string()
}

fn join_sub(title: &str, sub: &str) -> String {
    if sub.trim().is_empty() { clean(title) } else { clean(&format!("{} {}", title.trim(), sub.trim())) }
}

fn sanitize_filename(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c })
        .collect();
    let s = s.trim().trim_matches('.').trim();
    let s: String = s.chars().take(110).collect();
    if s.is_empty() { "song".to_string() } else { s }
}

fn pretty_difficulty(d: &str) -> String {
    let d = d.trim();
    if d.is_empty() {
        return "Chart".to_string();
    }
    match d.to_lowercase().as_str() {
        "smaniac" => "Challenge".to_string(),
        "basic" => "Easy".to_string(),
        low => {
            let mut c = low.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    }
}

struct BuiltChart {
    version: String,
    cols: usize,
    objs: Vec<Obj>,
    timing: Vec<(i64, f64)>,
}

fn build_chart(song: &sm::Song, chart: &sm::RawChart, warnings: &mut Vec<String>) -> Option<(usize, Vec<Obj>, Timing)> {
    let (cols, events) = sm::parse_notes(&chart.data);
    let label = format!("{} {}", chart.steps_type, chart.difficulty);
    if cols == 0 || events.is_empty() {
        return None;
    }
    if cols > 18 {
        warnings.push(format!("skipped {label}: {cols} columns is more than osu!mania supports (18)"));
        return None;
    }
    let mut tags = song.tags.clone();
    for (k, v) in &chart.local {
        tags.insert(k.clone(), v.clone());
    }
    let timing = Timing::new(&tags);

    let mut objs: Vec<Obj> = Vec::new();
    let mut open: Vec<Option<i64>> = vec![None; cols];
    let mut max_t = f64::NEG_INFINITY;
    let mut dropped = 0usize;

    for ev in events {
        if timing.is_unhittable(ev.beat) {
            dropped += 1;
            continue;
        }
        let t = timing.time_at(ev.beat);
        if t < max_t - 0.0005 {
            dropped += 1;
            continue;
        }
        max_t = max_t.max(t);
        let ms = (t * 1000.0).round() as i64;
        if ms < 0 {
            dropped += 1;
            continue;
        }
        match ev.kind {
            NoteKind::Tap => objs.push(Obj { col: ev.col, start: ms, end: None }),
            NoteKind::HoldHead => {
                if let Some(prev) = open[ev.col].replace(ms) {
                    objs.push(Obj { col: ev.col, start: prev, end: None });
                }
            }
            NoteKind::HoldTail => {
                if let Some(start) = open[ev.col].take() {
                    let end = if ms > start { Some(ms) } else { None };
                    objs.push(Obj { col: ev.col, start, end });
                }
            }
        }
    }
    for (col, o) in open.into_iter().enumerate() {
        if let Some(start) = o {
            objs.push(Obj { col, start, end: None });
        }
    }
    if dropped > 0 {
        warnings.push(format!("{label}: left out {dropped} warped/fake/negative-time notes"));
    }
    if objs.is_empty() {
        return None;
    }
    objs.sort_by_key(|o| (o.start, o.col));
    Some((cols, objs, timing))
}

#[allow(clippy::too_many_arguments)]
fn osu_text(
    built: &BuiltChart, audio_name: &str, bg_name: Option<&str>, title: &str, title_u: &str,
    artist: &str, artist_u: &str, creator: &str, source: &str, tags: &str, preview_ms: i64, opts: &Options,
) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "osu file format v14\n");
    let _ = writeln!(s, "[General]");
    let _ = writeln!(s, "AudioFilename: {audio_name}");
    let _ = writeln!(s, "AudioLeadIn: 0");
    let _ = writeln!(s, "PreviewTime: {preview_ms}");
    let _ = writeln!(s, "Countdown: 0");
    let _ = writeln!(s, "SampleSet: Soft");
    let _ = writeln!(s, "StackLeniency: 0.7");
    let _ = writeln!(s, "Mode: 3");
    let _ = writeln!(s, "LetterboxInBreaks: 0");
    let _ = writeln!(s, "SpecialStyle: 0");
    let _ = writeln!(s, "WidescreenStoryboard: 0\n");
    let _ = writeln!(s, "[Editor]\nDistanceSpacing: 1\nBeatDivisor: 4\nGridSize: 8\nTimelineZoom: 1\n");
    let _ = writeln!(s, "[Metadata]");
    let _ = writeln!(s, "Title:{title}\nTitleUnicode:{title_u}\nArtist:{artist}\nArtistUnicode:{artist_u}");
    let _ = writeln!(s, "Creator:{creator}\nVersion:{}\nSource:{source}\nTags:{tags}", built.version);
    let _ = writeln!(s, "BeatmapID:0\nBeatmapSetID:-1\n");
    let _ = writeln!(s, "[Difficulty]");
    let _ = writeln!(s, "HPDrainRate:{}\nCircleSize:{}\nOverallDifficulty:{}\nApproachRate:5", opts.hp, built.cols, opts.od);
    let _ = writeln!(s, "SliderMultiplier:1.4\nSliderTickRate:1\n");
    let _ = writeln!(s, "[Events]\n//Background and Video events");
    if let Some(bg) = bg_name {
        let _ = writeln!(s, "0,0,\"{bg}\",0,0");
    }
    let _ = writeln!(s, "\n[TimingPoints]");
    for &(ms, bpm) in &built.timing {
        let _ = writeln!(s, "{ms},{},4,1,0,60,1,0", 60000.0 / bpm);
    }
    let _ = writeln!(s, "\n[HitObjects]");
    let cols = built.cols as i64;
    for o in &built.objs {
        let x = ((2 * o.col as i64 + 1) * 512) / (2 * cols);
        match o.end {
            Some(end) => { let _ = writeln!(s, "{x},192,{},128,0,{end}:0:0:0:0:", o.start); }
            None => { let _ = writeln!(s, "{x},192,{},1,0,0:0:0:0:", o.start); }
        }
    }
    s
}

pub fn convert_file(path: &Path, opts: &Options, used_names: &mut HashSet<String>) -> Result<Report, String> {
    let bytes = fs::read(path).map_err(|e| format!("couldn't read the file: {e}"))?;
    let text = sm::decode_text(&bytes);
    let song = if ext_lower(path) == "ssc" { sm::parse_ssc(&text) } else { sm::parse_sm(&text) };
    if song.charts.is_empty() {
        return Err("no charts (#NOTES) found in this file".into());
    }
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "song".into());
    let mut warnings = Vec::new();

    let music_tag = song.tag("MUSIC").to_string();
    let audio = find_file(dir, &music_tag).or_else(|| guess_audio(dir)).ok_or_else(|| {
        if music_tag.is_empty() {
            "no audio file found next to the chart".to_string()
        } else {
            format!("audio file \"{music_tag}\" not found next to the chart")
        }
    })?;
    if !matches!(ext_lower(&audio).as_str(), "mp3" | "ogg" | "wav") {
        warnings.push(format!("audio is .{}; osu! officially supports .mp3/.ogg/.wav, so convert it if there is no sound", ext_lower(&audio)));
    }
    let audio_name = audio.file_name().unwrap().to_string_lossy().to_string();

    let bg = find_background(&song, dir);
    let bg_name = bg.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).filter(|n| !n.contains('"'));

    let title_u = { let t = join_sub(song.tag("TITLE"), song.tag("SUBTITLE")); if t.is_empty() { clean(&stem) } else { t } };
    let title = {
        let tt = if song.tag("TITLETRANSLIT").is_empty() { song.tag("TITLE") } else { song.tag("TITLETRANSLIT") };
        let st = if song.tag("SUBTITLETRANSLIT").is_empty() { song.tag("SUBTITLE") } else { song.tag("SUBTITLETRANSLIT") };
        let t = join_sub(tt, st);
        if t.is_empty() { title_u.clone() } else { t }
    };
    let artist_u = { let a = clean(song.tag("ARTIST")); if a.is_empty() { "Unknown Artist".to_string() } else { a } };
    let artist = { let a = clean(song.tag("ARTISTTRANSLIT")); if a.is_empty() { artist_u.clone() } else { a } };
    let creator = { let c = clean(song.tag("CREDIT")); if c.is_empty() { "sm2osz".to_string() } else { c } };
    let mut tags = String::from("stepmania sm2osz");
    let genre = clean(song.tag("GENRE"));
    if !genre.is_empty() {
        tags.push(' ');
        tags.push_str(&genre);
    }
    let preview_ms = song.tag("SAMPLESTART").parse::<f64>().map(|s| (s * 1000.0).round() as i64).unwrap_or(-1);

    let mut built: Vec<BuiltChart> = Vec::new();
    let mut versions: HashSet<String> = HashSet::new();
    for chart in &song.charts {
        if chart.steps_type.to_lowercase().contains("lights") {
            continue;
        }
        let Some((cols, objs, timing)) = build_chart(&song, chart, &mut warnings) else { continue };
        let diff = pretty_difficulty(&chart.difficulty);
        let meter = chart.meter.trim();
        let lv = if meter.is_empty() { String::new() } else { format!(" Lv.{meter}") };
        let desc = clean(&chart.description).replace(['[', ']'], "");
        let base = if diff == "Edit" && !desc.is_empty() { format!("{cols}K Edit - {desc}{lv}") } else { format!("{cols}K {diff}{lv}") };
        let mut version = base.clone();
        let mut n = 2;
        while !versions.insert(version.to_lowercase()) {
            version = format!("{base} #{n}");
            n += 1;
        }
        built.push(BuiltChart { version, cols, objs, timing: timing.timing_points() });
    }
    if built.is_empty() {
        return Err("no playable charts found (all were empty or unsupported)".into());
    }
    built.sort_by_key(|b| b.cols);

    let mut base = sanitize_filename(&format!("{artist} - {title}"));
    let mut n = 2;
    while !used_names.insert(base.to_lowercase()) {
        base = format!("{} ({n})", sanitize_filename(&format!("{artist} - {title}")));
        n += 1;
    }
    fs::create_dir_all(&opts.out_dir).map_err(|e| format!("couldn't create the output folder: {e}"))?;
    let osz_path = opts.out_dir.join(format!("{base}.osz"));

    let write = || -> Result<(), String> {
        let file = File::create(&osz_path).map_err(|e| format!("couldn't create {}: {e}", osz_path.display()))?;
        let mut zw = ZipWriter::new(file);
        let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let store = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let zerr = |e: zip::result::ZipError| format!("zip error: {e}");
        let ioerr = |e: std::io::Error| format!("write error: {e}");
        for b in &built {
            let osu = osu_text(b, &audio_name, bg_name.as_deref(), &title, &title_u, &artist, &artist_u, &creator, "", &tags, preview_ms, opts);
            let fname = sanitize_filename(&format!("{artist} - {title} ({creator}) [{}]", b.version));
            zw.start_file(format!("{fname}.osu"), deflate).map_err(zerr)?;
            zw.write_all(osu.as_bytes()).map_err(ioerr)?;
        }
        let audio_bytes = fs::read(&audio).map_err(|e| format!("couldn't read audio: {e}"))?;
        zw.start_file(audio_name.clone(), store).map_err(zerr)?;
        zw.write_all(&audio_bytes).map_err(ioerr)?;
        if let (Some(bg), Some(name)) = (&bg, &bg_name) {
            if let Ok(bytes) = fs::read(bg) {
                zw.start_file(name.clone(), store).map_err(zerr)?;
                zw.write_all(&bytes).map_err(ioerr)?;
            }
        }
        zw.finish().map_err(zerr)?;
        Ok(())
    };
    if let Err(e) = write() {
        let _ = fs::remove_file(&osz_path);
        return Err(e);
    }
    Ok(Report { osz: osz_path, charts: built.len(), warnings })
}

pub fn run_all(inputs: &[PathBuf], opts: &Options, report: &dyn Fn(Msg)) {
    let files = collect_charts(inputs);
    let total = files.len();
    if total == 0 {
        report(Msg::Log(Level::Warn, "No .sm files found there. Drop a song folder (or a whole pack) that contains .sm files.".into()));
        report(Msg::Done { created: 0, failed: 0, out_dir: opts.out_dir.clone() });
        return;
    }
    report(Msg::Log(Level::Info, format!("Found {total} song file(s). Converting...")));
    let mut used = HashSet::new();
    let (mut created, mut failed) = (0, 0);
    for (i, f) in files.iter().enumerate() {
        report(Msg::Progress { done: i, total });
        let label = f.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| f.display().to_string());
        match catch_unwind(AssertUnwindSafe(|| convert_file(f, opts, &mut used))) {
            Ok(Ok(r)) => {
                created += 1;
                let name = r.osz.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                report(Msg::Log(Level::Ok, format!("OK  {name}  ({} difficulties)", r.charts)));
                for w in r.warnings {
                    report(Msg::Log(Level::Warn, format!("    note: {w}")));
                }
            }
            Ok(Err(e)) => {
                failed += 1;
                report(Msg::Log(Level::Error, format!("FAILED  {label}: {e}")));
            }
            Err(_) => {
                failed += 1;
                report(Msg::Log(Level::Error, format!("FAILED  {label}: unexpected error while reading this chart")));
            }
        }
    }
    report(Msg::Progress { done: total, total });
    report(Msg::Done { created, failed, out_dir: opts.out_dir.clone() });
}