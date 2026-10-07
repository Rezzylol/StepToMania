#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod convert;
mod sm;

use convert::{Level, Msg, Options};
use eframe::egui::{self, Color32, RichText};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

struct App {
    out_dir: Option<PathBuf>,
    od: f32,
    hp: f32,
    log: Vec<(Level, String)>,
    rx: Option<Receiver<Msg>>,
    running: bool,
    progress: (usize, usize),
    last_out: Option<PathBuf>,
    summary: Option<(usize, usize)>,
    pending: Vec<PathBuf>,
}

impl App {
    fn new(pending: Vec<PathBuf>) -> Self {
        App {
            out_dir: None, od: 8.0, hp: 8.0,
            log: vec![(Level::Info, "Ready. Drop a StepMania song folder or a whole pack onto this window.".into())],
            rx: None, running: false, progress: (0, 0), last_out: None, summary: None, pending,
        }
    }

    fn start(&mut self, ctx: &egui::Context, inputs: Vec<PathBuf>) {
        if self.running {
            self.log.push((Level::Warn, "Still working - wait for the current conversion to finish.".into()));
            return;
        }
        let out_dir = self.out_dir.clone().unwrap_or_else(|| convert::default_output_dir(&inputs));
        let opts = Options { od: self.od, hp: self.hp, out_dir: out_dir.clone() };
        let (tx, rx) = mpsc::channel();
        let ctx2 = ctx.clone();
        self.rx = Some(rx);
        self.running = true;
        self.summary = None;
        self.progress = (0, 0);
        self.last_out = Some(out_dir);
        self.log.push((Level::Info, "-----".into()));
        std::thread::spawn(move || {
            convert::run_all(&inputs, &opts, &|m| {
                let _ = tx.send(m);
                ctx2.request_repaint();
            });
        });
    }

    fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Log(l, s) => self.log.push((l, s)),
                Msg::Progress { done, total } => self.progress = (done, total),
                Msg::Done { created, failed, out_dir } => {
                    self.running = false;
                    self.summary = Some((created, failed));
                    self.last_out = Some(out_dir);
                }
            }
        }
    }
}

fn open_folder(p: &Path) {
    let _ = std::fs::create_dir_all(p);
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if !self.pending.is_empty() {
            let p = std::mem::take(&mut self.pending);
            self.start(ctx, p);
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            self.start(ctx, dropped);
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("StepMania to osu!mania").size(26.0).strong());
            ui.label("Turns .sm charts into .osz beatmaps you can play in osu!mania.");
            ui.add_space(10.0);

            let (fill, stroke) = if hovering {
                (Color32::from_rgb(30, 70, 50), Color32::from_rgb(90, 220, 140))
            } else {
                (ui.visuals().extreme_bg_color, ui.visuals().widgets.inactive.fg_stroke.color)
            };
            egui::Frame::none().fill(fill).stroke(egui::Stroke::new(2.0, stroke)).rounding(12.0).inner_margin(20.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    if self.running {
                        ui.label(RichText::new("Converting...").size(20.0).strong());
                        ui.add_space(6.0);
                        let (d, t) = self.progress;
                        let frac = if t == 0 { 0.0 } else { d as f32 / t as f32 };
                        ui.add(egui::ProgressBar::new(frac).text(format!("{d} / {t}")).animate(true));
                    } else {
                        let text = if hovering { "Release to convert!" } else { "Drag and drop a folder here" };
                        ui.label(RichText::new(text).size(22.0).strong());
                        ui.label("A single song folder, or a whole pack of songs");
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let w = ui.available_width();
                            ui.add_space((w - 330.0).max(0.0) / 2.0);
                            if ui.button("  Choose folder...  ").clicked() {
                                if let Some(p) = rfd::FileDialog::new().set_title("Choose a song or pack folder").pick_folder() {
                                    self.start(ctx, vec![p]);
                                }
                            }
                            if ui.button("  Choose .sm file(s)...  ").clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .set_title("Choose StepMania charts")
                                    .add_filter("StepMania charts", &["sm", "ssc"])
                                    .pick_files()
                                {
                                    self.start(ctx, p);
                                }
                            }
                        });
                    }
                });
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label("Save .osz files to:");
                match &self.out_dir {
                    Some(p) => ui.label(RichText::new(p.display().to_string()).strong()),
                    None => ui.label(RichText::new("a folder called \"osz output\" next to what you drop").italics()),
                };
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!self.running, |ui| {
                    if ui.button("Change...").clicked() {
                        if let Some(p) = rfd::FileDialog::new().set_title("Where should the .osz files go?").pick_folder() {
                            self.out_dir = Some(p);
                        }
                    }
                    if self.out_dir.is_some() && ui.button("Back to automatic").clicked() {
                        self.out_dir = None;
                    }
                });
            });

            egui::CollapsingHeader::new("Options").show(ui, |ui| {
                ui.add(egui::Slider::new(&mut self.od, 0.0..=10.0).step_by(0.5).text("Overall Difficulty (judgement strictness)"));
                ui.add(egui::Slider::new(&mut self.hp, 0.0..=10.0).step_by(0.5).text("HP Drain"));
            });

            ui.add_space(6.0);
            if let Some((created, failed)) = self.summary {
                let (msg, color) = if created > 0 && failed == 0 {
                    (format!("Done! Made {created} .osz file(s)."), Color32::from_rgb(90, 220, 140))
                } else if created > 0 {
                    (format!("Made {created} .osz file(s); {failed} failed (see below)."), Color32::from_rgb(240, 190, 80))
                } else {
                    ("Nothing was converted (see below).".to_string(), Color32::from_rgb(240, 100, 100))
                };
                ui.label(RichText::new(msg).size(18.0).strong().color(color));
                if created > 0 {
                    ui.horizontal(|ui| {
                        if ui.button("Open output folder").clicked() {
                            if let Some(p) = &self.last_out {
                                open_folder(p);
                            }
                        }
                        ui.label("Then double-click an .osz to import it into osu!");
                    });
                }
                ui.add_space(4.0);
            }

            ui.separator();
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                for (level, line) in &self.log {
                    let color = match level {
                        Level::Info => ui.visuals().text_color(),
                        Level::Ok => Color32::from_rgb(90, 220, 140),
                        Level::Warn => Color32::from_rgb(240, 190, 80),
                        Level::Error => Color32::from_rgb(240, 100, 100),
                    };
                    ui.label(RichText::new(line).color(color).monospace());
                }
            });
        });
    }
}

fn main() -> eframe::Result {
    let pending: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).filter(|p| p.exists()).collect();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 640.0])
            .with_min_inner_size([520.0, 480.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("StepMania to osu!mania Converter", options, Box::new(move |_cc| Ok(Box::new(App::new(pending)))))
}