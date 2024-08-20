use std::{collections::HashSet, sync::mpsc::Receiver, time::Duration};

use super::{FieldFlags, StateMonitor};
use crate::{screen, server, utils};

static CHECKBOXES: [&str; 5] = ["Mirror", "Flip", "Sine", "Random Flip", "Sync Offset"];
const X_SCALAR: i32 = 10;

#[derive(PartialEq, Eq, Hash)]
enum Fields {
    Select,
    Edit,
    Add,
    Delete,
    Duplicate,
    AdjustOne,
    AdjustAll,
    CopyNext,
    CopyPrev,
}

pub struct PatternsPanel {
    state: PatternsState,
    prop_cache: Vec<String>,
    pattern_cache: Vec<(String, String)>,
    monitor: StateMonitor<PatternsState>,
    modified: FieldFlags<Fields>,
    receiver: Receiver<(usize, i32)>,
    commands: HashSet<&'static str>,
    current_section_length: i32,
    scroll_to: Option<usize>,
}

struct Pattern {
    prop: String,
    position: i32,
    size: i32,
    spacing: i32,
    x: i32,
    freq: i32,
    amp: i32,
    offset: i32,
    flags: u32,
}

#[derive(Default, Hash)]
struct PatternsState {
    section: usize,
    index: usize,
    prop: usize,
    position: i32,
    size: i32,
    spacing: i32,
    x: i32,
    freq: i32,
    amp: i32,
    offset: i32,
    flags: u32,
    clicks: u8,
}

impl From<&Pattern> for PatternsState {
    fn from(value: &Pattern) -> Self {
        PatternsState {
            section: 0,
            index: 0,
            prop: 0,
            clicks: 0,
            position: value.position,
            size: value.size,
            spacing: value.spacing,
            x: value.x / X_SCALAR,
            freq: value.freq,
            amp: value.amp / X_SCALAR,
            offset: value.offset,
            flags: value.flags,
        }
    }
}

impl screen::Panel for PatternsPanel {}

impl PatternsPanel {
    pub fn new(receiver: Receiver<(usize, i32)>) -> Self {
        PatternsPanel {
            prop_cache: Vec::new(),
            pattern_cache: Vec::new(),
            state: Default::default(),
            monitor: StateMonitor::new(),
            modified: FieldFlags::new(),
            receiver,
            commands: HashSet::new(),
            current_section_length: 0,
            scroll_to: None,
        }
    }

    fn update_sliders(&mut self) {
        if self.pattern_cache.is_empty() {
            self.state = Default::default();
        } else if let Some(p) = Self::extract_pattern_data(&self.pattern_cache[self.state.index].0)
        {
            let prop = self
                .prop_cache
                .iter()
                .position(|x| *x == p.prop)
                .unwrap_or(0);
            let (section, index, clicks) = {
                let s = &self.state;
                (s.section, s.index, s.clicks)
            };
            self.state = PatternsState::from(&p);
            self.state.prop = prop;
            self.state.section = section;
            self.state.index = index;
            self.state.clicks = clicks;
        }
    }

    fn format_pattern_setter(&self) -> String {
        let s = &self.state;
        format!(
            "pattern-set {} {} {} {} {} {} {} {} {} {} {}",
            s.section,
            s.index,
            self.prop_cache[s.prop],
            s.position,
            s.size,
            s.spacing,
            s.x * X_SCALAR,
            s.freq,
            s.amp * X_SCALAR,
            s.offset,
            s.flags
        )
    }

    fn format_pattern_data(index: usize, pattern: &Pattern) -> String {
        format!("#{} :: {} ({})", index, pattern.prop, pattern.position)
    }

    fn extract_pattern_data(pattern: &str) -> Option<Pattern> {
        let parts = pattern
            .split(char::is_whitespace)
            .skip(3)
            .map(str::trim)
            .collect::<Vec<&str>>();
        let prop = String::from(parts[0]);
        let mut numbers = Vec::new();

        for n in parts.iter().skip(1).take(parts.len() - 1) {
            numbers.push(n.parse::<i32>());
        }

        let flags = parts[parts.len() - 1].parse::<u32>();
        if numbers.iter().all(Result::is_ok) && flags.is_ok() {
            let r = numbers
                .into_iter()
                .map(Result::unwrap)
                .collect::<Vec<i32>>();
            Some(Pattern {
                prop,
                position: r[0],
                size: r[1],
                spacing: r[2],
                x: r[3],
                freq: r[4],
                amp: r[5],
                offset: r[6],
                flags: flags.unwrap(),
            })
        } else {
            None
        }
    }
}

impl screen::CommandHandler for PatternsPanel {
    fn should_handle(&self, command: &str) -> bool { self.commands.contains(command) }

    fn handle(&mut self, response: &server::Response) -> utils::UnitResult {
        const EMPTY_SIGNAL: &str = "<EMPTY>";
        let (err, cmd, _, resp) = response.decompose();
        if cmd == "package-props" {
            self.prop_cache.clear();
            if !err && !resp.starts_with(EMPTY_SIGNAL) {
                self.prop_cache.extend(resp.lines().map(str::trim).map(String::from));
            }
        } else if cmd == "pattern-add" {
            self.scroll_to.replace(self.pattern_cache.len());
        } else if cmd == "pattern-duplicate" {
            self.scroll_to.replace(self.state.index + 1);
        } else if cmd == "pattern-list" {
            self.pattern_cache.clear();
            if !err && !resp.starts_with(EMPTY_SIGNAL) {
                self.pattern_cache
                    .extend(
                        resp.lines()
                            .map(str::trim)
                            .enumerate()
                            .filter_map(|(i, x)| {
                                Self::extract_pattern_data(x)
                                    .map(|p| (String::from(x), Self::format_pattern_data(i, &p)))
                            }),
                    );
            }
            let s = &mut self.state;
            let n = if let Some(scroll) = self.scroll_to.take() {
                scroll
            } else {
                s.index
            };
            s.prop = s.prop.clamp(0, self.prop_cache.len().saturating_sub(1));
            s.index = n.clamp(0, self.pattern_cache.len().saturating_sub(1));
            self.update_sliders();
        }
        Ok(())
    }
}

impl screen::StateSync for PatternsPanel {
    fn initialize_state(&mut self, _send: &mut dyn FnMut(&str)) {
        self.commands.extend(vec![
            "package-props",
            "section-list",
            "pattern-list",
            "pattern-add",
            "pattern-delete",
            "pattern-duplicate",
            "pattern-copy-all",
            "pattern-adjust",
            "pattern-set",
        ]);
    }

    fn update_state(&mut self) {
        self.monitor.update(&self.state);
        if let Ok((s, len)) = self.receiver.try_recv() {
            self.current_section_length = len;
            if s != self.state.section {
                self.state.section = s;
            }
            self.state.position = self.state.position.clamp(0, self.current_section_length);
        }
    }

    fn request_state(&self, send: &mut dyn FnMut(&str)) {
        send("package-props");
        send(&format!("pattern-list {}", self.state.section));
    }

    fn write_state(&mut self, send: &mut dyn FnMut(&str)) {
        if self.monitor.time_elapsed(Duration::from_millis(120)) {
            let section = self.state.section;
            let pattern = self.state.index;
            let (mut setter, mut sliders) = (false, false);
            for field in self.modified.drain() {
                match field {
                    Fields::Add => {
                        send(&format!("pattern-add {}", section));
                    }
                    Fields::Delete => {
                        send(&format!("pattern-delete {} {}", section, pattern));
                    }
                    Fields::Duplicate => {
                        send(&format!("pattern-duplicate {} {}", section, pattern));
                    }
                    Fields::AdjustOne => {
                        send(&format!("pattern-adjust {} {}", section, pattern));
                    }
                    Fields::AdjustAll => {
                        for (i, _) in self.pattern_cache.iter().enumerate() {
                            send(&format!("pattern-adjust {} {}", section, i));
                        }
                    }
                    Fields::CopyPrev => {
                        send(&format!("pattern-copy-all {} {}", section, section - 1));
                    }
                    Fields::CopyNext => {
                        send(&format!("pattern-copy-all {} {}", section, section + 1));
                    }
                    Fields::Select => {
                        sliders = true;
                    }
                    Fields::Edit => {
                        setter = true;
                    }
                }
            }
            if setter {
                send(&self.format_pattern_setter());
            }
            if sliders {
                self.update_sliders();
            }
            send(&format!("pattern-list {}", section));
            self.monitor.sleep();
        }
    }
}

impl screen::Render for PatternsPanel {
    fn render(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        use egui::{ComboBox, Frame, Label, RichText as RT, Sense, Vec2};
        Frame::none()
            .inner_margin(Vec2::from([8.0, 16.0]))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::from([8.0, 8.0]);
                ui.horizontal(|ui| {
                    ui.add_space(64.0);
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
                    if ui.button(RT::new("Adjust").size(14.0)).clicked() {
                        self.modified.flag(Fields::AdjustOne);
                        self.state.clicks = self.state.clicks.overflowing_add(1).0;
                    }
                    if ui.button(RT::new("Adjust All").size(14.0)).clicked() {
                        self.modified.flag(Fields::AdjustAll);
                        self.state.clicks = self.state.clicks.overflowing_add(1).0;
                    }
                    if ui.button(RT::new("Copy Prev").size(14.0)).clicked() {
                        self.modified.flag(Fields::CopyPrev);
                        self.state.clicks = self.state.clicks.overflowing_add(1).0;
                    }
                    if ui.button(RT::new("Copy Next").size(14.0)).clicked() {
                        self.modified.flag(Fields::CopyNext);
                        self.state.clicks = self.state.clicks.overflowing_add(1).0;
                    }
                });
                ui.horizontal(|ui| {
                    let spacing = 16.0;
                    let combo_width = 180.0;
                    ui.add_space(spacing);
                    ui.add(Label::new(RT::new("Pattern").size(14.0)));
                    ui.add_space(spacing);
                    ComboBox::from_id_source("pattern_selector")
                        .selected_text(if self.pattern_cache.is_empty() {
                            "<empty>"
                        } else {
                            &self.pattern_cache[self.state.index].1
                        })
                        .width(combo_width)
                        .show_ui(ui, |ui| {
                            for (i, p) in self.pattern_cache.iter().enumerate() {
                                if ui.add(Label::new(RT::new(&p.1).size(14.0)).truncate(true).sense(Sense::click())).clicked() {
                                    self.state.index = i;
                                    self.modified.flag(Fields::Select);
                                }
                            }
                        });
                    ui.add_space(spacing);
                    ui.add(Label::new(RT::new("Prop").size(14.0)));
                    ui.add_space(spacing);
                    ComboBox::from_id_source("prop_selector")
                        .selected_text(if self.prop_cache.is_empty() {
                            "<empty>"
                        } else {
                            &self.prop_cache[self.state.prop]
                        })
                        .width(combo_width)
                        .show_ui(ui, |ui| {
                            for (i, p) in self.prop_cache.iter().enumerate() {
                                if ui.add(Label::new(RT::new(p).size(14.0)).truncate(true).sense(Sense::click())).clicked() {
                                    self.state.prop = i;
                                    self.modified.flag(Fields::Edit);
                                }
                            }
                        });
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::from([8.0, 2.0]);
                    let slider_size = [500.0, 24.0];
                    let no_format = Option::<fn(i32) -> String>::None;
                    let mut sliders = false;
                    nofmt::pls! {
                        sliders |= Self::precision_slider("Position", slider_size, &mut self.state.position, 0..=self.current_section_length, no_format, ui);
                        sliders |= Self::precision_slider("Size", slider_size, &mut self.state.size, 0..=500, no_format, ui);
                        sliders |= Self::precision_slider("Spacing", slider_size, &mut self.state.spacing, 1..=50, no_format, ui);
                        sliders |= Self::precision_slider("X", slider_size, &mut self.state.x, -1000..=1000, Some(|x| format!("{}", x * X_SCALAR)), ui);
                        sliders |= Self::precision_slider("Frequency", slider_size, &mut self.state.freq, 1..=255, no_format, ui);
                        sliders |= Self::precision_slider("Amplitude", slider_size, &mut self.state.amp, 0..=500, Some(|x| format!("{}", x * X_SCALAR)), ui);
                        sliders |= Self::precision_slider("Mirror Offset", slider_size, &mut self.state.offset, -50..=50, no_format, ui);
                    }
                    if sliders {
                        self.modified.flag(Fields::Edit);
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(96.0);
                        for (i, name) in CHECKBOXES.iter().enumerate() {
                            let mut v = (self.state.flags >> i & 1) != 0;
                            if ui.checkbox(&mut v, RT::new(*name).size(14.0)).clicked() {
                                self.modified.flag(Fields::Edit);
                                if v {
                                    self.state.flags |= 1 << i;
                                } else {
                                    self.state.flags &= !(1 << i);
                                }
                            }
                        }
                    });
                })
            });
    }
}
