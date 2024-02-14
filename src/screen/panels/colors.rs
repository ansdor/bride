use std::{collections::HashSet, time::Duration};

use super::StateMonitor;
use crate::{screen, server, utils};

pub struct ColorsPanel {
    state: ColorsPanelState,
    commands: HashSet<&'static str>,
    monitor: StateMonitor<ColorsPanelState>,
    modified: u32,
}

#[derive(Hash)]
struct ColorsPanelState {
    colors: [[u8; 4]; 8],
}

static COLOR_NAMES: [&str; 8] = [
    "ground #0",
    "ground #1",
    "road #0",
    "road #1",
    "stripes #0",
    "stripes #1",
    "markings",
    "fog",
];

impl screen::Panel for ColorsPanel {}

impl ColorsPanel {
    pub fn new() -> Self {
        let state = ColorsPanelState {
            colors: [[0; 4]; 8],
        };
        let monitor = StateMonitor::new();
        ColorsPanel {
            state,
            commands: HashSet::new(),
            monitor,
            modified: 0,
        }
    }
}

impl screen::CommandHandler for ColorsPanel {
    fn should_handle(&self, command: &str) -> bool { self.commands.contains(command) }

    fn handle(&mut self, contents: &server::Response) -> utils::UnitResult {
        let (err, cmd, _, resp) = contents.decompose();
        if !err && cmd == "color-list" {
            for (index, line) in resp.lines().map(str::trim).enumerate() {
                let color = u32::from_str_radix(line, 16)?;
                self.state.colors[index] = color.to_be_bytes();
            }
        }
        Ok(())
    }
}

impl screen::StateSync for ColorsPanel {
    fn initialize_state(&mut self, _send: &mut dyn FnMut(&str)) {
        self.commands.insert("color-list");
    }

    fn update_state(&mut self) { self.monitor.update(&self.state); }

    fn request_state(&self, send: &mut dyn FnMut(&str)) { send("color-list"); }

    fn write_state(&mut self, send: &mut dyn FnMut(&str)) {
        if self.monitor.time_elapsed(Duration::from_millis(100)) {
            for i in 0..self.state.colors.len() {
                if self.modified >> i & 1 != 0 {
                    let color = u32::from_be_bytes(self.state.colors[i]);
                    send(format!("color-set {} 0x{:x}", i, color).as_str());
                }
            }
            self.modified = 0;
            send("view-preview");
            self.monitor.sleep();
        }
    }
}

impl screen::Render for ColorsPanel {
    fn render(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        egui::Frame::none()
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(640.0 - 16.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::from([16.0, 4.0]);
                    ui.add_space(81.0);
                    egui::Grid::new("colors_grid").show(ui, |ui| {
                        for row in 0..2 {
                            for col in 0..4 {
                                let i = col * 2 + row % 2;
                                ui.label(COLOR_NAMES[i]);
                                if ui
                                    .color_edit_button_srgba_unmultiplied(&mut self.state.colors[i])
                                    .changed()
                                {
                                    self.modified |= 1 << i;
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
            });
    }
}
