use std::{collections::HashSet, sync::mpsc::Sender, time::Duration};

use super::{FieldFlags, StateMonitor};
use crate::{screen, server, utils::UnitResult};

const LENGTH_SCALAR: i32 = 25;

#[derive(PartialEq, Eq, Hash)]
enum Fields {
    Select,
    Edit,
    Add,
    Delete,
    Duplicate,
    MoveUp,
    MoveDown,
}

pub struct SectionsPanel {
    cache: Vec<(String, String)>,
    state: SectionsState,
    commands: HashSet<&'static str>,
    monitor: StateMonitor<SectionsState>,
    modified: FieldFlags<Fields>,
    pattern_sender: Sender<(usize, i32)>,
    view_sender: Sender<(i32, i32)>,
    metrics: String,
    scroll_to: Option<usize>,
    last_move: Option<isize>,
}

#[derive(Default, Hash)]
struct SectionsState {
    selected: usize,
    length: i32,
    curve: i32,
    slope: i32,
    split: i32,
    clicks: u8,
}

impl screen::Panel for SectionsPanel {}

impl SectionsPanel {
    pub fn new(pattern_sender: Sender<(usize, i32)>, view_sender: Sender<(i32, i32)>) -> Self {
        SectionsPanel {
            cache: Vec::new(),
            state: Default::default(),
            commands: HashSet::new(),
            monitor: StateMonitor::new(),
            modified: FieldFlags::new(),
            pattern_sender,
            view_sender,
            metrics: String::from("-"),
            scroll_to: None,
            last_move: None,
        }
    }

    fn update_sliders(&mut self) {
        let selected = self.state.selected;
        let clicks = self.state.clicks;
        if self.cache.is_empty() {
            self.state = Default::default();
        } else if let Some((length, curve, slope, split)) =
            Self::extract_section_data(&self.cache[selected].0)
        {
            self.state = SectionsState {
                length: length / LENGTH_SCALAR,
                curve,
                slope,
                split,
                selected,
                clicks,
            }
        } else {
            self.state = Default::default();
        }
    }

    fn format_section_data(index: usize, section: &str) -> Option<String> {
        if let Some((length, curve, slope, split)) = Self::extract_section_data(section) {
            let curve_type = match curve {
                0 => "straight",
                x if x < -2 => "sharp left",
                x if x > 2 => "sharp right",
                x if x < 0 => "wide left",
                x if x > 0 => "wide right",
                _ => unreachable!(),
            };
            let slope_type = match slope {
                0 => "flat",
                x if x > 0 => "upward",
                x if x < 0 => "downward",
                _ => unreachable!(),
            };
            let num_lanes = match split {
                0 => "3 lanes",
                1 => "4 lanes",
                2 => "5 lanes",
                _ => "dual tracks",
            };
            let b = format!(
                "[{:2}] {:11} {:12} {:9} {:12}",
                index,
                format!("length {}", length),
                curve_type,
                slope_type,
                num_lanes
            );
            Some(b)
        } else {
            None
        }
    }

    fn extract_section_data(section: &str) -> Option<(i32, i32, i32, i32)> {
        let split = section
            .split(char::is_whitespace)
            .skip(2)
            .collect::<Vec<&str>>();
        if let (Ok(length), Ok(curve), Ok(slope), Ok(split)) = (
            split[0].parse::<i32>(),
            split[1].parse::<i32>(),
            split[2].parse::<i32>(),
            split[3].parse::<i32>(),
        ) {
            Some((length, curve, slope, split))
        } else {
            None
        }
    }
}

impl screen::CommandHandler for SectionsPanel {
    fn should_handle(&self, command: &str) -> bool { self.commands.contains(command) }

    fn handle(&mut self, response: &server::Response) -> UnitResult {
        let (err, cmd, _, resp) = response.decompose();
        if !err {
            if cmd == "section-add" {
                self.scroll_to.replace(self.cache.len());
            } else if cmd == "section-list" {
                use super::EMPTY_SIGNAL;
                self.cache.clear();
                if !resp.starts_with(EMPTY_SIGNAL) {
                    resp.lines()
                        .map(str::trim)
                        .map(String::from)
                        .enumerate()
                        .for_each(|(i, s)| {
                            if let Some(f) = Self::format_section_data(i, &s) {
                                self.cache.push((s, f));
                            }
                        });
                }
                let n = if let Some(s) = self.scroll_to.take() {
                    s
                } else {
                    self.state.selected
                };
                let limit = self.cache.len().saturating_sub(1);
                self.state.selected = n.clamp(0, limit);
                self.update_sliders();
            } else if cmd == "section-metrics" {
                self.metrics.clear();
                for line in resp.lines().map(str::trim) {
                    if let Some((field, val)) = line.split_once(char::is_whitespace) {
                        self.metrics.push_str(match field {
                            "view" => "Viewing Section #",
                            "segments" => "\tTotal Segments: ",
                            "curve" => "\tTotal Curve: ",
                            "slope" => "\tTotal Slope: ",
                            _ => "?????",
                        });
                        self.metrics.push_str(val);
                    }
                }
            } else if cmd == "section-duplicate" {
                self.scroll_to.replace(self.state.selected + 1);
            } else if cmd == "section-move" {
                if let Some(d) = self.last_move.take() {
                    self.scroll_to.replace(self.state.selected.saturating_add_signed(d));
                }
            }
        }
        Ok(())
    }
}

impl screen::StateSync for SectionsPanel {
    fn initialize_state(&mut self, _send: &mut dyn FnMut(&str)) {
        self.commands.extend(vec![
            "section-add",
            "section-list",
            "section-metrics",
            "section-duplicate",
            "section-move",
        ]);
    }

    fn update_state(&mut self) {
        self.monitor.update(&self.state);
        if let Err(msg) = self
            .pattern_sender
            .send((self.state.selected, self.state.length * LENGTH_SCALAR))
        {
            println!("{msg}");
        }
    }

    fn request_state(&self, send: &mut dyn FnMut(&str)) {
        send("section-list");
        send("section-metrics");
    }

    fn write_state(&mut self, send: &mut dyn FnMut(&str)) {
        if self.monitor.time_elapsed(Duration::from_millis(80)) {
            let mut request = false;
            let mut sliders = false;
            let s = &self.state.selected;
            for field in self.modified.drain() {
                request = true;
                match field {
                    Fields::Select => sliders = true,
                    Fields::Add => send("section-add"),
                    Fields::Delete => send(&format!("section-delete {}", s)),
                    Fields::Duplicate => send(&format!("section-duplicate {}", s)),
                    Fields::MoveUp => {
                        send(&format!("section-move {} -1", s));
                        let _ = self.last_move.replace(-1);
                    }
                    Fields::MoveDown => {
                        send(&format!("section-move {} 1", s));
                        let _ = self.last_move.replace(1);
                    }
                    Fields::Edit => {
                        let s = &self.state;
                        send(&format!(
                            "section-set {} {} {} {} {}",
                            s.selected,
                            s.length * LENGTH_SCALAR,
                            s.curve,
                            s.slope,
                            s.split
                        ));
                    }
                };
            }
            if sliders {
                self.update_sliders();
            }
            if request {
                send("section-list");
                send("section-metrics");
                send("view-preview");
            }
            self.monitor.sleep();
        }
    }
}

impl screen::Render for SectionsPanel {
    fn render(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        use egui::{ComboBox, Frame, Label, RichText as RT, Sense, TextStyle, Vec2};
        Frame::none()
            .inner_margin(Vec2::from([8.0, 16.0]))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::from([4.0, 10.0]);
                ui.label(RT::new(&self.metrics).size(14.0));
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.add(Label::new(RT::new("Section").size(14.0)));
                    ui.add_space(16.0);
                    ComboBox::from_id_source("section_selector")
                        .selected_text(if self.cache.is_empty() {
                            "<empty>"
                        } else {
                            &self.cache[self.state.selected].1
                        })
                        .width(400.0)
                        .show_ui(ui, |ui| {
                            if !self.cache.is_empty() {
                                for (i, s) in self.cache.iter().enumerate() {
                                    if ui
                                        .add(
                                            Label::new(RT::new(&s.1).text_style(TextStyle::Monospace).size(12.0))
                                                .sense(Sense::click()),
                                        )
                                        .clicked()
                                    {
                                        self.state.selected = i;
                                        self.modified.flag(Fields::Select);
                                    }
                                }
                            }
                        });
                    ui.add_space(8.0);
                    if ui.button(RT::new("View").size(14.0)).clicked() {
                        let view_z: i32 = self.cache.iter().map(|x| {
                            if let Some(d) = Self::extract_section_data(&x.0) {
                                d.0
                            } else {
                                0
                            }
                        }).take(self.state.selected).sum();
                        if let Err(msg) = self.view_sender.send((0, view_z)) {
                            eprintln!("{msg}");
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.add_space(256.0);
                    ui.spacing_mut().item_spacing = Vec2::from([8.0, 4.0]);
                    if ui.button(RT::new("Add").size(14.0)).clicked() {
                        self.modified.flag(Fields::Add);
                        self.state.clicks = self.state.clicks.overflowing_add(1).0;
                    }
                    if ui.button(RT::new("Delete").size(14.0)).clicked() {
                        self.modified.flag(Fields::Delete);
                        self.state.clicks = self.state.clicks.overflowing_add(1).0;
                    }
                    if ui.button(RT::new("Duplicate").size(14.0)).clicked() {
                        self.modified.flag(Fields::Duplicate);
                        self.state.clicks = self.state.clicks.overflowing_add(1).0;
                    }
                    if ui.button(RT::new("Move Up").size(14.0)).clicked() {
                        self.modified.flag(Fields::MoveUp);
                        self.state.clicks = self.state.clicks.overflowing_add(1).0;
                    }
                    if ui.button(RT::new("Move Down").size(14.0)).clicked() {
                        self.modified.flag(Fields::MoveDown);
                        self.state.clicks = self.state.clicks.overflowing_add(1).0;
                    }
                });

                const SLIDER_SIZE: [f32; 2] = [500.0, 24.0];
                const NO_FORMATTER: Option<fn(i32) -> String> = Option::<fn(i32) -> String>::None;

                ui.spacing_mut().item_spacing = Vec2::from([4.0, 2.0]);
                let mut sliders = false;
                nofmt::pls! {
                    sliders |= Self::precision_slider("Length", SLIDER_SIZE, &mut self.state.length, 0..=(1250/LENGTH_SCALAR), Some(|s| format!("{}", s * LENGTH_SCALAR)), ui, );
                    sliders |= Self::precision_slider("Curve", SLIDER_SIZE, &mut self.state.curve, -4..=4, NO_FORMATTER, ui, );
                    sliders |= Self::precision_slider("Slope", SLIDER_SIZE, &mut self.state.slope, -4..=4, NO_FORMATTER, ui, );
                    sliders |= Self::precision_slider("Split", SLIDER_SIZE, &mut self.state.split, 0..=9, Some(|s| format!("{}", s - 3)), ui, );
                }
                if sliders {
                    self.modified.flag(Fields::Edit);
                }
            });
    }
}
