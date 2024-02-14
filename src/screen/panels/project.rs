use std::collections::{HashSet, VecDeque};

use crate::{screen, server, utils};

#[derive(Debug)]
enum ProjectAction {
    New,
    Refresh,
    Load(usize),
    Save(Option<String>),
    Delete(usize),
}

#[derive(Clone, Copy)]
enum DialogWindow {
    Projects,
    Discard,
    Delete,
    Overwrite,
    Filename,
}

#[derive(Default)]
pub struct ProjectPanel {
    //server synced data
    commands: HashSet<&'static str>,
    project_cache: Vec<String>,
    project_file_name: Option<String>,
    //state
    execute: Option<()>,
    selected: usize,
    confirmed: Option<bool>,
    actions: VecDeque<ProjectAction>,
    name_buffer: String,
    //sub windows
    dialog_window: Option<DialogWindow>,
}

impl screen::Panel for ProjectPanel {}

impl ProjectPanel {
    pub fn new() -> Self { Default::default() }

    fn push_action(&mut self, action: ProjectAction) {
        self.confirmed.take();
        self.actions.push_back(action);
        self.execute.replace(());
    }

    pub fn extra_window<R>(
        ctx: &egui::Context, title: &str, size: [f32; 2], stroke_color: egui::Color32,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::Response
    where
        Self: Sized, {
        let offset = ctx.screen_rect().height() / 2.0 - size[1] / 2.0;
        let dialog =
            egui::Area::new("extra_window").anchor(egui::Align2::CENTER_TOP, [0.0, offset]);
        let r = dialog
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_hex("#212228").unwrap())
                    .inner_margin(16.0)
                    .stroke((2.0, stroke_color))
                    .show(ui, |ui| {
                        ui.set_width(size[0]);
                        ui.label(egui::RichText::new(title).size(20.0));
                        ui.separator();
                        ui.vertical(add_contents)
                    })
                    .response
            })
            .response;
        ctx.move_to_top(dialog.layer());
        r
    }
}

impl screen::CommandHandler for ProjectPanel {
    fn should_handle(&self, command: &str) -> bool { self.commands.contains(command) }

    fn handle(&mut self, response: &server::Response) -> utils::UnitResult {
        const EMPTY_SIGNAL: &str = "<EMPTY>";
        let (err, cmd, args, resp) = response.decompose();
        if cmd == "project-list" {
            self.project_cache.clear();
            if resp.trim() != EMPTY_SIGNAL {
                self.project_cache
                    .extend(resp.lines().map(str::trim).map(String::from));
            }
        } else if cmd == "project-file-name" {
            if resp.trim() != EMPTY_SIGNAL {
                let s = String::from(resp.trim());
                self.project_file_name.replace(s.clone());
                self.name_buffer = s.clone();
            } else {
                self.name_buffer.clear();
                self.project_file_name.take();
            }
        } else if cmd == "project-new" || cmd == "project-load" {
            if err && self.confirmed.is_none() {
                self.dialog_window.replace(DialogWindow::Discard);
            } else {
                self.actions.pop_front();
                self.confirmed.take();
            }
        } else if cmd == "project-save" {
            if err && !args.is_empty() && self.confirmed.is_none() {
                self.dialog_window.replace(DialogWindow::Overwrite);
            } else {
                self.actions.pop_front();
                self.confirmed.take();
            }
        } else if cmd == "project-delete" {
            self.actions.pop_front();
            self.confirmed.take();
        }
        Ok(())
    }
}

impl screen::StateSync for ProjectPanel {
    fn initialize_state(&mut self, _send: &mut dyn FnMut(&str)) {
        self.commands.extend(vec![
            "project-new",
            "project-load",
            "project-save",
            "project-delete",
            "project-list",
            "project-file-name",
        ]);
    }

    fn update_state(&mut self) {}

    fn request_state(&self, send: &mut dyn FnMut(&str)) {
        send("project-list");
        send("project-file-name");
    }

    fn write_state(&mut self, send: &mut dyn FnMut(&str)) {
        if self.execute.take().is_some() && !self.actions.is_empty() {
            let mut pop_next_action = false;
            match &self.actions[0] {
                ProjectAction::New => match self.confirmed {
                    None => {
                        send("project-new");
                    }
                    Some(c) => send(&format!("project-new {}", utils::bool_string(c))),
                },
                ProjectAction::Refresh => {
                    send("project-list");
                    pop_next_action = true;
                }
                ProjectAction::Load(n) => {
                    if self.project_cache.len() > *n {
                        match self.confirmed {
                            None => send(&format!("project-load \"{}\"", self.project_cache[*n])),
                            Some(c) => send(&format!(
                                "project-load \"{}\" {}",
                                self.project_cache[*n],
                                utils::bool_string(c)
                            )),
                        };
                    }
                }
                ProjectAction::Save(None) => {
                    send("project-save");
                }
                ProjectAction::Save(Some(n)) => match self.confirmed {
                    None => send(&format!("project-save \"{}\"", n)),
                    Some(c) => send(&format!("project-save \"{}\" {}", n, utils::bool_string(c))),
                },
                ProjectAction::Delete(n) => match self.confirmed {
                    None => {
                        self.dialog_window.replace(DialogWindow::Delete);
                    }
                    Some(true) => {
                        send(&format!("project-delete \"{}\"", self.project_cache[*n]));
                    }
                    Some(false) => {
                        pop_next_action = true;
                    }
                },
            }
            if pop_next_action {
                self.actions.pop_front();
            }
        }
    }
}

impl screen::Render for ProjectPanel {
    fn render(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        use egui::{Button, Color32, Frame, Label, RichText as RT, ScrollArea, TextStyle, Vec2};
        Frame::none()
            .inner_margin(Vec2::from([8.0, 8.0]))
            .show(ui, |ui| {
                ui.set_height(32.0);
                ui.set_width(640.0 - 24.0);
                ui.horizontal_centered(|ui| {
                    let char_limit = 28;
                    let project_name = match &self.project_file_name {
                        Some(name) => name,
                        None => "<unsaved>",
                    };
                    ui.add_sized(
                        [ui.available_width() / 3.0, 32.0],
                        Label::new(
                            RT::new(if project_name.len() > char_limit {
                                &project_name[0..char_limit]
                            } else {
                                project_name
                            })
                            .size(20.0),
                        ),
                    );
                    let new_button = Button::new(RT::new("New Track").size(14.0));
                    if ui.add_sized([64.0, 32.0], new_button).clicked() {
                        self.push_action(ProjectAction::New);
                    }
                    ui.add_space(12.0);
                    let load_button = Button::new(RT::new("Load Track").size(14.0));
                    if ui.add_sized([64.0, 32.0], load_button).clicked() {
                        self.push_action(ProjectAction::Refresh);
                        self.dialog_window.replace(DialogWindow::Projects);
                    }
                    ui.add_space(12.0);
                    let rename_button = Button::new(RT::new("Save As...").size(14.0));
                    if ui.add_sized([64.0, 32.0], rename_button).clicked() {
                        self.dialog_window.replace(DialogWindow::Filename);
                    }
                    ui.add_space(12.0);
                    let save_button = Button::new(RT::new("Save Track").size(14.0));
                    if ui.add_sized([96.0, 32.0], save_button).clicked() {
                        match &self.project_file_name {
                            Some(_) => {
                                self.push_action(ProjectAction::Save(None));
                            }
                            None => {
                                self.dialog_window.replace(DialogWindow::Filename);
                            }
                        }
                    }
                })
            });

        let mut close_dialog = false;
        if let Some(dialog) = self.dialog_window {
            let (title, size, color) = match dialog {
                DialogWindow::Projects => ("Projects", [400.0, 300.0], egui::Color32::GRAY),
                DialogWindow::Discard | DialogWindow::Overwrite | DialogWindow::Delete => {
                    ("Warning", [360.0, 200.0], egui::Color32::DARK_RED)
                }
                DialogWindow::Filename => ("Project Name", [360.0, 200.0], egui::Color32::GRAY),
            };
            Self::extra_window(ctx, title, size, color, |ui| match dialog {
                DialogWindow::Projects => {
                    let row_height = ui.text_style_height(&TextStyle::Monospace);
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(row_height * 10.0)
                        .show(ui, |ui| {
                            for (i, p) in self.project_cache.iter().enumerate() {
                                let label = RT::new(p).text_style(TextStyle::Monospace).size(14.0);
                                let pred = self.selected == i;
                                if ui.selectable_label(pred, label).clicked() {
                                    self.selected = i;
                                }
                            }
                        });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(Button::new("Load Project").min_size(Vec2::from([96.0, 32.0])))
                            .clicked()
                        {
                            self.push_action(ProjectAction::Load(self.selected));
                            close_dialog = true;
                        }
                        if ui
                            .add(Button::new("Refresh List").min_size(Vec2::from([64.0, 32.0])))
                            .clicked()
                        {
                            self.push_action(ProjectAction::Refresh);
                        }
                        if ui
                            .add(
                                Button::new("Delete Project")
                                    .fill(Color32::from_rgb(0x99, 0x33, 0x22))
                                    .min_size(Vec2::from([72.0, 32.0])),
                            )
                            .clicked()
                        {
                            self.push_action(ProjectAction::Delete(self.selected));
                        }
                        if ui
                            .add(Button::new("Close").min_size(Vec2::from([64.0, 32.0])))
                            .clicked()
                        {
                            close_dialog = true;
                        }
                    });
                }
                DialogWindow::Discard | DialogWindow::Overwrite | DialogWindow::Delete => {
                    if matches!(dialog, DialogWindow::Overwrite) {
                        ui.label(
                            RT::new(format!(
                                "A file named '{}' already exists. Overwrite?",
                                self.name_buffer
                            ))
                            .size(16.0),
                        );
                    } else if matches!(dialog, DialogWindow::Delete) {
                        let index = match self.actions[0] {
                            ProjectAction::Delete(n) => n,
                            _ => unreachable!(),
                        };
                        ui.label(
                            RT::new(format!(
                                "Really delete project '{}'?",
                                self.project_cache[index]
                            ))
                            .size(16.0),
                        );
                    } else {
                        ui.label(
                            RT::new(
                                "Unsaved changes to the current project will be lost. Proceed?",
                            )
                            .size(16.0),
                        );
                    }
                    ui.add_space(32.0);
                    ui.horizontal(|ui| {
                        ui.add_space(49.0);
                        if ui
                            .add(Button::new("Confirm").min_size(Vec2::from([96.0, 32.0])))
                            .clicked()
                        {
                            self.confirmed.replace(true);
                            self.execute.replace(());
                            close_dialog = true;
                        }
                        ui.add_space(64.0);
                        if ui
                            .add(Button::new("Cancel").min_size(Vec2::from([96.0, 32.0])))
                            .clicked()
                        {
                            self.confirmed.replace(false);
                            self.execute.replace(());
                            close_dialog = true;
                        }
                    });
                }
                DialogWindow::Filename => {
                    ui.vertical_centered(|ui| {
                        ui.label(RT::new("File name for this project:").size(14.0));
                        ui.add_space(16.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.name_buffer)
                                .lock_focus(true)
                                .desired_width(192.0),
                        );
                        ui.add_space(16.0);
                        ui.horizontal(|ui| {
                            ui.add_space(92.0);
                            if ui
                                .add(Button::new("Save").min_size(Vec2::from([64.0, 32.0])))
                                .clicked()
                                && !self.name_buffer.is_empty()
                            {
                                self.push_action(ProjectAction::Save(Some(
                                    self.name_buffer.clone(),
                                )));
                                close_dialog = true;
                            }
                            ui.add_space(32.0);
                            if ui
                                .add(Button::new("Cancel").min_size(Vec2::from([64.0, 32.0])))
                                .clicked()
                            {
                                close_dialog = true;
                            }
                        });
                    });
                }
            });
        }
        if close_dialog {
            self.dialog_window.take();
        }
    }
}
