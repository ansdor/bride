use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use super::{FieldFlags, StateMonitor};
use crate::{screen, server, utils};

#[derive(PartialEq, Eq, Hash)]
enum HeaderFields {
    Package,
    Name,
    Background,
    Texture,
    Flags,
    PlayTest,
    RandomSeed,
}

pub struct HeaderPanel {
    state: HeaderState,
    commands: HashSet<&'static str>,
    monitor: StateMonitor<HeaderState>,
    content_lists: HashMap<&'static str, Vec<String>>,
    modified: FieldFlags<HeaderFields>,
    autosave: bool,
}

#[derive(Default, Hash)]
struct HeaderState {
    package: String,
    name: String,
    background: String,
    texture: String,
    flags: u32,
    random_seed: i64,
    clicks: u8,
}

static FLAG_LABELS: [&str; 10] = [
    "rain", "snow", "stardust", "bubbles", "vaccuum", "low vis", "(unused)", "(unused)", "fog",
    "darkness",
];

impl screen::Panel for HeaderPanel {}

impl HeaderPanel {
    pub fn new() -> Self {
        HeaderPanel {
            state: Default::default(),
            commands: HashSet::new(),
            monitor: StateMonitor::new(),
            content_lists: HashMap::new(),
            modified: FieldFlags::new(),
            autosave: true,
        }
    }
}

impl screen::CommandHandler for HeaderPanel {
    fn should_handle(&self, command: &str) -> bool { self.commands.contains(command) }

    fn handle(&mut self, contents: &server::Response) -> utils::UnitResult {
        let (err, cmd, _, resp) = contents.decompose();
        let error = Err("Failed to parse server response.".into());
        if !err {
            if cmd == "header-get" {
                for line in resp.lines().map(str::trim) {
                    let (field, value) = match line.split_once(char::is_whitespace) {
                        Some((f, v)) => (f, v),
                        None => return error,
                    };
                    match field {
                        "package" => self.state.package = value.to_owned(),
                        "name" => self.state.name = value.to_owned(),
                        "background" => self.state.background = value.to_owned(),
                        "texture" => self.state.texture = value.to_owned(),
                        "random-seed" => {
                            if let Ok(v) = value.parse::<i64>() {
                                self.state.random_seed = v
                            } else {
                                return error;
                            }
                        }
                        "flags" => {
                            if let Ok(v) = value.parse::<u32>() {
                                self.state.flags = v
                            } else {
                                return error;
                            }
                        }
                        _ => {}
                    };
                }
            } else {
                let key = match cmd {
                    "package-list" => "packages",
                    "package-backgrounds" => "backgrounds",
                    "package-textures" => "textures",
                    "package-props" => "props",
                    _ => return error,
                };
                let list = self.content_lists.entry(key).or_default();
                list.clear();
                list.extend(resp.lines().map(str::trim).map(String::from).collect::<Vec<String>>());
            }
        }
        Ok(())
    }
}

impl screen::StateSync for HeaderPanel {
    fn initialize_state(&mut self, _send: &mut dyn FnMut(&str)) {
        self.commands.extend(vec![
            "header-get",
            "package-list",
            "package-backgrounds",
            "package-textures",
            "package-props",
        ]);
    }

    fn update_state(&mut self) { self.monitor.update(&self.state); }

    fn request_state(&self, send: &mut dyn FnMut(&str)) {
        send("package-list");
        send("package-backgrounds");
        send("package-textures");
        send("package-props");
        send("header-get");
    }

    fn write_state(&mut self, send: &mut dyn FnMut(&str)) {
        let mut extras = false;
        let mut play_test = false;
        if self.monitor.time_elapsed(Duration::from_millis(48)) {
            let mut request = false;
            for field in self.modified.drain() {
                request = true;
                extras |= matches!(field, HeaderFields::Package);
                if matches!(field, HeaderFields::PlayTest) {
                    play_test = true;
                    continue;
                }
                send(
                    match field {
                        HeaderFields::Package => format!("package-load \"{}\"", self.state.package),
                        HeaderFields::Name => format!("header-name-set \"{}\"", self.state.name),
                        HeaderFields::Background => {
                            format!("header-background-set \"{}\"", self.state.background)
                        }
                        HeaderFields::Texture => {
                            format!("header-texture-set \"{}\"", self.state.texture)
                        }
                        HeaderFields::Flags => {
                            format!("header-flags-set {:#012b}", self.state.flags)
                        }
                        HeaderFields::RandomSeed => String::from("header-new-random-seed"),
                        HeaderFields::PlayTest => unreachable!(),
                    }
                    .as_str(),
                );
            }
            if request {
                send("header-get");
            }
            if extras {
                send("package-backgrounds");
                send("package-textures");
                send("package-props");
            }
            if play_test {
                if self.autosave {
                    send("project-save");
                }
                send("track-play-test");
            }
            self.monitor.sleep();
        }
    }
}

impl screen::Render for HeaderPanel {
    fn render(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        use egui::{Button, ComboBox, Frame, Grid, Label, RichText as RT, Sense, TextEdit, Vec2};
        Frame::none().inner_margin(8.0).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::from([16.0, 0.0]);
            ui.horizontal(|ui| {
                ui.label("Track Name");
                if ui
                    .add(
                        TextEdit::singleline(&mut self.state.name)
                            .lock_focus(true)
                            .desired_width(192.0),
                    )
                    .changed()
                {
                    self.modified.flag(HeaderFields::Name);
                }
                ui.add_space(16.0);
                ui.label("Package");
                ComboBox::from_id_source("package")
                    .selected_text(&self.state.package)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        let mut selected_index = None;
                        if let Some(pkgs) = &self.content_lists.get("packages") {
                            for (i, p) in pkgs.iter().enumerate() {
                                if ui.add(Label::new(p).sense(Sense::click())).clicked() {
                                    selected_index = Some(i);
                                }
                            }
                            if let Some(i) = selected_index {
                                if self.state.package != pkgs[i] {
                                    self.state.package = pkgs[i].clone();
                                    self.modified.flag(HeaderFields::Package);
                                }
                            }
                        }
                    });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Background");
                ComboBox::from_id_source("background")
                    .selected_text(&self.state.background)
                    .width(192.0)
                    .show_ui(ui, |ui| {
                        let mut selected_index = None;
                        if let Some(bgs) = &self.content_lists.get("backgrounds") {
                            for (i, p) in bgs.iter().enumerate() {
                                if ui.add(Label::new(p).sense(Sense::click())).clicked() {
                                    selected_index = Some(i);
                                }
                            }
                            if let Some(i) = selected_index {
                                if self.state.background != bgs[i] {
                                    self.state.background = bgs[i].clone();
                                    self.modified.flag(HeaderFields::Background);
                                }
                            }
                        }
                    });
                ui.add_space(16.0);
                ui.label("Texture");
                ComboBox::from_id_source("texture")
                    .selected_text(&self.state.texture)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        let mut selected_index = None;
                        if let Some(bgs) = &self.content_lists.get("textures") {
                            for (i, p) in bgs.iter().enumerate() {
                                if ui.add(Label::new(p).sense(Sense::click())).clicked() {
                                    selected_index = Some(i);
                                }
                            }
                            if let Some(i) = selected_index {
                                if self.state.texture != bgs[i] {
                                    self.state.texture = bgs[i].clone();
                                    self.modified.flag(HeaderFields::Texture);
                                }
                            }
                        }
                    });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.vertical(|ui| {
                    Grid::new("track_flags_grid").show(ui, |ui| {
                        for row in 0..2 {
                            for col in 0..5 {
                                let i = row * 5 + col;
                                let mut current = self.state.flags >> i & 1 != 0;
                                if ui.checkbox(&mut current, FLAG_LABELS[i]).changed() {
                                    self.modified.flag(HeaderFields::Flags);
                                    if current {
                                        self.state.flags |= 1 << i;
                                    } else {
                                        self.state.flags &= !(1 << i);
                                    }
                                }
                            }
                            ui.end_row();
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let new_random_seed = Button::new(RT::new("New Random Seed").size(14.0));
                        if ui.add_sized([160.0, 24.0], new_random_seed).clicked() {
                            self.modified.flag(HeaderFields::RandomSeed);
                            self.state.clicks = self.state.clicks.overflowing_add(1).0;
                        }
                        ui.add_space(8.0);
                        ui.label(format!("{:#x}", self.state.random_seed));
                    });
                });
                ui.add_space(56.0);
                ui.vertical(|ui| {
                    let test_button = Button::new(RT::new("Play Test").size(14.0));
                    if ui.add_sized([128.0, 32.0], test_button).clicked() {
                        self.modified.flag(HeaderFields::PlayTest);
                        self.state.clicks = self.state.clicks.overflowing_add(1).0;
                    }
                    ui.add_space(4.0);
                    ui.checkbox(&mut self.autosave, "Auto Save");
                })
            });
        });
    }
}
