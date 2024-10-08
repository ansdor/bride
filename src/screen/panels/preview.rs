use std::{borrow::Cow, collections::HashSet, time::Duration};

use base64::Engine;

use super::{FieldFlags, StateMonitor};
use crate::{screen, server, utils};

const IDLE_PREVIEW_TIME: u64 = 480;
const CHANGE_PREVIEW_TIME: u64 = 80;
const PREVIEW_SIZE: [f32; 2] = [640.0, 360.0];
const SLIDER_SCALE: i32 = 10;

#[derive(PartialEq, Eq, Hash)]
enum Fields {
    View,
    Overview,
    Reverse,
}

pub struct PreviewPanel {
    state: PreviewPanelState,
    commands: HashSet<&'static str>,
    image_data: Option<Vec<u8>>,
    monitor: StateMonitor<PreviewPanelState>,
    modified: FieldFlags<Fields>,
}

#[derive(Default, Hash)]
struct PreviewPanelState {
    view_x: i32,
    view_z: i32,
    overview: bool,
    reverse: bool,
}

impl PreviewPanel {
    pub fn new() -> Self {
        PreviewPanel {
            state: PreviewPanelState::default(),
            commands: HashSet::new(),
            image_data: None,
            monitor: StateMonitor::new(),
            modified: FieldFlags::new(),
        }
    }
}

impl screen::Panel for PreviewPanel {}

impl screen::CommandHandler for PreviewPanel {
    fn should_handle(&self, command: &str) -> bool { self.commands.contains(command) }

    fn handle(&mut self, contents: &server::Response) -> utils::UnitResult {
        let (err, cmd, args, resp) = contents.decompose();
        if !err {
            if cmd == "view-state-info" {
                for line in resp.lines().map(str::trim) {
                    if let Some((field, val)) = line.split_once(char::is_whitespace) {
                        match field {
                            "position" => {
                                let coords = val
                                    .split_once(char::is_whitespace)
                                    .map(|(x, z)| (x.parse::<i32>(), z.parse::<i32>()));
                                if let Some((Ok(x), Ok(z))) = coords {
                                    self.state.view_x = x / SLIDER_SCALE;
                                    self.state.view_z = z / SLIDER_SCALE;
                                }
                            }
                            "reverse" => self.state.reverse = val.starts_with("#t"),
                            "overview" => self.state.overview = val.starts_with("#t"),
                            _ => {}
                        }
                    }
                }
            } else if cmd == "view-position" {
                let coords = args
                    .split_once(char::is_whitespace)
                    .map(|(x, z)| (x.parse::<i32>(), z.parse::<i32>()));
                if let Some((Ok(x), Ok(z))) = coords {
                    self.state.view_x = x / SLIDER_SCALE;
                    self.state.view_z = z / SLIDER_SCALE;
                }
            } else if cmd == "view-preview" && self.image_data.is_none() {
                let mut image_data = Vec::with_capacity(8192);
                base64::engine::general_purpose::STANDARD.decode_vec(resp, &mut image_data)?;
                self.image_data.replace(image_data);
            }
        }
        Ok(())
    }
}

impl screen::StateSync for PreviewPanel {
    fn initialize_state(&mut self, send: &mut dyn FnMut(&str)) {
        self.commands
            .extend(vec!["view-preview", "view-position", "view-state-info"]);
        let (w, h) = (PREVIEW_SIZE[0] as u32, PREVIEW_SIZE[1] as u32);
        send(format!("view-preview-size {} {}", w, h).as_str());
        send("view-state-info");
    }

    fn update_state(&mut self) { self.monitor.update(&self.state); }

    fn request_state(&self, send: &mut dyn FnMut(&str)) {
        send("view-preview");
        send("view-state-info");
    }

    fn write_state(&mut self, send: &mut dyn FnMut(&str)) {
        let t = Duration::from_millis(CHANGE_PREVIEW_TIME);
        if self.monitor.time_elapsed(t) {
            let mut extra = false;
            for field in self.modified.drain() {
                extra |= matches!(field, Fields::View);
                send(
                    match field {
                        Fields::View => {
                            format!(
                                "view-position {} {}",
                                self.state.view_x * SLIDER_SCALE,
                                self.state.view_z * SLIDER_SCALE
                            )
                        }
                        Fields::Reverse => {
                            format!("track-reverse {}", utils::bool_string(self.state.reverse))
                        }
                        Fields::Overview => {
                            format!("view-overview {}", utils::bool_string(self.state.overview))
                        }
                    }
                    .as_str(),
                );
            }
            send("view-preview");
            if extra {
                send("section-metrics");
            }
            self.monitor
                .advance(Duration::from_millis(IDLE_PREVIEW_TIME) - t);
        }
    }
}

impl screen::Render for PreviewPanel {
    fn render(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        use egui::{Frame, Image, Vec2};
        Frame::none().inner_margin(0.0).show(ui, |ui| {
            ui.set_width(640.0);
            ui.add(
                if self.image_data.is_some() {
                    ctx.forget_image("bytes://preview");
                    let image_data = self.image_data.take().unwrap();
                    Image::from_bytes(Cow::Borrowed("bytes://preview"), image_data)
                } else {
                    Image::from_uri("bytes://preview")
                }
                .fit_to_exact_size(Vec2::from(PREVIEW_SIZE)),
            );

            const SLIDER_SIZE: [f32; 2] = [512.0, 16.0];
            let formatter = |n| format!("{}", n * SLIDER_SCALE);

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                if Self::precision_slider(
                    "View X",
                    SLIDER_SIZE,
                    &mut self.state.view_x,
                    -300..=300,
                    Some(formatter),
                    ui,
                ) {
                    self.modified.flag(Fields::View);
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                if Self::precision_slider(
                    "View Z",
                    SLIDER_SIZE,
                    &mut self.state.view_z,
                    0..=500,
                    Some(formatter),
                    ui,
                ) {
                    self.modified.flag(Fields::View);
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(241.0);
                if ui.checkbox(&mut self.state.overview, "Overview").changed() {
                    self.modified.flag(Fields::Overview);
                }
                ui.add_space(16.0);
                if ui.checkbox(&mut self.state.reverse, "Reverse").changed() {
                    self.modified.flag(Fields::Reverse);
                };
            });
            ui.add_space(8.0);
        });
    }
}
