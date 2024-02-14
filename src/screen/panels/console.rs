use std::{collections::{HashSet, VecDeque}, sync::mpsc::Receiver};

use crate::{screen, server, utils::UnitResult};

const ENTRY_LIMIT: usize = 300;

pub struct ConsolePanel {
    strict_excludes: HashSet<&'static str>,
    normal_excludes: HashSet<&'static str>,
    history: VecDeque<String>,
    command_buffer: String,
    command_send_flag: bool,
    expecting: Option<String>,
    receiver: Option<Receiver<String>>
}

impl ConsolePanel {
    pub fn new(receiver: Option<Receiver<String>>) -> Self {
        ConsolePanel {
            strict_excludes: HashSet::new(),
            normal_excludes: HashSet::new(),
            history: VecDeque::new(),
            command_buffer: String::new(),
            command_send_flag: false,
            expecting: None,
            receiver
        }
    }

    fn write_line(&mut self, msg: &str) {
        if self.history.len() >= ENTRY_LIMIT {
            self.history.pop_front();
        }
        self.history.push_back(String::from(msg));
    }
}

impl screen::CommandHandler for ConsolePanel {
    fn should_handle(&self, command: &str) -> bool { !self.strict_excludes.contains(command) }

    fn handle(&mut self, contents: &server::Response) -> UnitResult {
        let (err, cmd, args, resp) = contents.decompose();
        if let Some(e) = &self.expecting {
            if e == cmd {
                self.expecting.take();
            }
        } else if !err && self.normal_excludes.contains(cmd) {
            return Ok(());
        }
        let mut buffer = String::new();
        let status = if err { "ERROR" } else { "OK" };
        buffer.push_str(&format!("* {} ({}) => [{}]", cmd, args, status));
        if !resp.is_empty() {
            buffer.push('\n');
            buffer.push_str(resp);
        }
        self.write_line(&buffer);
        Ok(())
    }
}

impl screen::Render for ConsolePanel {
    fn render(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        egui::Frame::none().inner_margin(8.0).show(ui, |ui| {
            ui.set_width(640.0 - 16.0);
            ui.set_height(223.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Console").text_style(egui::TextStyle::Monospace));
                ui.separator();
                let style = egui::TextStyle::Monospace;
                let row_height = ui.text_style_height(&style);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .max_height(row_height * 12.0)
                    .show_rows(ui, row_height, self.history.len(), |ui, row_range| {
                        for row in row_range {
                            ui.label(
                                egui::RichText::new(&self.history[row])
                                    .text_style(egui::TextStyle::Monospace),
                            );
                        }
                    });
                ui.add_space(2.0);
                let console_input = ui.add(
                    egui::TextEdit::singleline(&mut self.command_buffer)
                        .lock_focus(true)
                        .desired_width(f32::INFINITY),
                );
                if console_input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.command_send_flag = true;
                    ui.memory_mut(|m| {
                        m.request_focus(console_input.id);
                    });
                }
            });
        });
    }
}

impl screen::StateSync for ConsolePanel {
    fn initialize_state(&mut self, _send: &mut dyn FnMut(&str)) {
        //these commands will never show up on the console
        self.strict_excludes = HashSet::from_iter(vec!["view-preview"]);
        //these won't show on the console in case of success
        self.normal_excludes = HashSet::from_iter(vec![
            "view-position",
            "view-overview",
            "track-reverse",
            "color-list",
            "view-preview-size",
            "package-list",
            "package-backgrounds",
            "package-textures",
            "package-props",
            "header-get",
            "section-list",
            "section-metrics",
            "project-list",
            "project-file-name",
            "pattern-list"
        ]);
    }

    fn update_state(&mut self) {
        let rx = self.receiver.take();
        if let Some(rx) = rx {
            while let Ok(msg) = rx.try_recv() {
                self.write_line(&msg);
            }
            self.receiver.replace(rx);
        }
    }

    fn request_state(&self, _send: &mut dyn FnMut(&str)) {}

    fn write_state(&mut self, send: &mut dyn FnMut(&str)) {
        if self.command_send_flag && !self.command_buffer.is_empty() {
            self.expecting = self
                .command_buffer
                .split(char::is_whitespace)
                .map(String::from)
                .next();
            send(&self.command_buffer);
            self.command_buffer.clear();
            self.command_send_flag = false;
        }
    }
}

impl screen::Panel for ConsolePanel {}
