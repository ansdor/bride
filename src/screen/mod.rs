use std::{
    collections::HashSet,
    mem,
    net::SocketAddr,
    ops::RangeInclusive,
    sync::mpsc::{self, Sender},
    time::{Duration, Instant},
};

use self::panels::{
    ColorsPanel, ConsolePanel, HeaderPanel, PatternsPanel, PreviewPanel, ProjectPanel,
    SectionsPanel,
};
use crate::{
    server::{self, CommandQueue},
    utils::UnitResult,
};

mod panels;

pub struct Screen {
    enabled: bool,
    resize_frame_skip: bool,
    queue: CommandQueue,
    state_reset: HashSet<&'static str>,
    panels: Vec<Box<dyn Panel>>,
    connection_timer: Instant,
    console: Option<Sender<String>>,
    port: u16,
}

pub trait StateSync {
    fn initialize_state(&mut self, send: &mut dyn FnMut(&str));
    fn update_state(&mut self);
    fn request_state(&self, send: &mut dyn FnMut(&str));
    fn write_state(&mut self, send: &mut dyn FnMut(&str));
}

pub trait CommandHandler {
    fn should_handle(&self, command: &str) -> bool;
    fn handle(&mut self, response: &server::Response) -> UnitResult;
}

pub trait Render {
    fn render(&mut self, ctx: &egui::Context, ui: &mut egui::Ui);

    fn precision_slider(
        label: &str, size: [f32; 2], state: &mut i32, range: RangeInclusive<i32>,
        formatter: Option<impl Fn(i32) -> String>, ui: &mut egui::Ui,
    ) -> bool
    where
        Self: Sized, {
        let (min, max) = (*range.start(), *range.end());
        ui.horizontal(|ui| {
            use egui::{style::HandleShape, Label, Slider, Vec2};
            ui.spacing_mut().item_spacing = Vec2::from([4.0, 2.0]);
            let mut r = false;
            ui.add_sized([size[0] * 0.15, size[1]], Label::new(label));
            if ui.button("-").clicked() && *state > min {
                *state -= 1;
                r = true;
            }
            let prev_slider_size = ui.spacing().slider_width;
            ui.spacing_mut().slider_width = size[0] * 0.90;
            let slider = Slider::new(state, range)
                .clamp_to_range(true)
                .show_value(false)
                .handle_shape(HandleShape::Rect { aspect_ratio: 2.0 });
            if ui.add_sized(size, slider).changed() {
                r = true;
            }
            ui.spacing_mut().slider_width = prev_slider_size;
            if ui.button("+").clicked() && *state < max {
                *state += 1;
                r = true;
            }
            ui.add_sized(
                [size[0] * 0.10, size[1]],
                Label::new(if let Some(f) = formatter {
                    f(*state)
                } else {
                    format!("{:?}", state)
                }),
            );
            r
        })
        .inner
    }
}

pub trait Panel: Render + CommandHandler + StateSync {}

impl Screen {
    pub fn new(port: u16, _cc: &eframe::CreationContext<'_>) -> Self {
        Screen {
            connection_timer: Instant::now() - Duration::from_secs(10),
            queue: CommandQueue::new(),
            panels: Vec::new(),
            state_reset: HashSet::new(),
            enabled: false,
            resize_frame_skip: false,
            console: None,
            port,
        }
    }

    fn print(&mut self, msg: &str) {
        if let Some(c) = &self.console {
            if let Err(e) = c.send(String::from(msg)) {
                println!("{e}");
            }
        }
    }

    fn initialize(&mut self) {
        //comands in this list will trigger a state request
        self.state_reset.clear();
        self.state_reset.extend([
            "undo",
            "redo",
            "project-new",
            "project-load",
            "project-save",
            "project-delete",
        ]);
        //channel used by the sections and patterns panels
        let (t0, r0) = mpsc::channel::<(usize, i32)>();
        //channel used to send messages to the console window
        let (t1, r1) = mpsc::channel::<String>();
        //channel used to send data from sections to view
        let (t2, r2) = mpsc::channel::<(i32, i32)>();
        self.panels = vec![
            Box::new(ProjectPanel::new()),
            Box::new(HeaderPanel::new()),
            Box::new(SectionsPanel::new(t0, t2)),
            Box::new(PatternsPanel::new(r0)),
            Box::new(ColorsPanel::new()),
            Box::new(PreviewPanel::new(r2)),
            Box::new(ConsolePanel::new(Some(r1))),
        ];
        //create the console channel
        self.console.replace(t1);
        //initialize local state and sync with server
        self.panels.iter_mut().for_each(|p| {
            p.initialize_state(&mut |x| self.queue.send(x));
        });
    }

    fn global_update(&mut self) -> UnitResult {
        if self.panels.is_empty() {
            self.initialize();
        } else {
            if self.queue.connected() {
                if let Err(msg) = self.queue.update() {
                    self.print(&format!("[ERROR] {msg}"));
                    self.enabled = false;
                    let prev = mem::replace(&mut self.queue, CommandQueue::new());
                    if let Err(msg) = prev.disconnect() {
                        self.print(&format!("[ERROR] {msg}"));
                    }
                } else {
                    self.enabled = !self.queue.paused();
                    if let Some(r) = self.queue.receive() {
                        //in this scope, r is guaranteed to be
                        //either Success or Error, never Nothing
                        let (err, id, _, _) = r.decompose();
                        //check if this command should trigger
                        //a state request from the panels
                        if !err && self.state_reset.contains(id) {
                            self.panels
                                .iter_mut()
                                .for_each(|x| x.request_state(&mut |x| self.queue.send(x)));
                        }
                        for p in self.panels.iter_mut().filter(|x| x.should_handle(id)) {
                            p.handle(&r)?;
                        }
                    }
                    for p in self.panels.iter_mut() {
                        p.write_state(&mut |x| self.queue.send(x));
                    }
                }
            } else if self.connection_timer.elapsed() > Duration::from_secs(3) {
                let address = SocketAddr::from(([127, 0, 0, 1], self.port));
                if let Err(e) = self.queue.connect(&address) {
                    self.print(&format!("[ERROR] Failed to connect to server: {e}"));
                    self.connection_timer = Instant::now();
                } else {
                    self.print("[OK] Connected to server.");
                    //on successful connection, trigger state request
                    for p in self.panels.iter_mut() {
                        p.request_state(&mut |x| self.queue.send(x));
                    }
                }
            }
            self.panels.iter_mut().for_each(|x| x.update_state());
        }
        Ok(())
    }
}

impl eframe::App for Screen {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Err(msg) = self.global_update() {
            eprintln!("{msg}");
        }
        const LEFT_SIDE_PANELS: usize = 4;
        let current_scale = ctx.zoom_factor();
        let (rw, rh) = crate::WINDOW_SIZE;
        let (sw, sh) = (
            ctx.screen_rect().width() * current_scale,
            ctx.screen_rect().height() * current_scale,
        );
        let interface_scale = (sw / rw).min(sh / rh);
        if self.resize_frame_skip {
            //this fixes a bug that sometimes happens when rescaling the window
            self.resize_frame_skip = false;
        } else {
            ctx.set_zoom_factor(interface_scale);
            self.resize_frame_skip = true;
        }
        egui::Window::new("bride")
            .frame(egui::Frame::none().fill(egui::Color32::from_hex("#111218").unwrap()))
            .resizable(false)
            .movable(false)
            .collapsible(false)
            .title_bar(false)
            .enabled(self.enabled)
            .show(ctx, |ui| {
                if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z)) {
                    self.queue.send(if ui.input(|i| i.modifiers.shift) {
                        "redo"
                    } else {
                        "undo"
                    });
                }
                ui.spacing_mut().item_spacing = egui::Vec2::from([0.0, 0.0]);
                ui.set_width(ctx.screen_rect().width());
                ui.set_height(ctx.screen_rect().height());
                ui.add_space(ui.available_height() / 2.0 - rh / 2.0);
                ui.horizontal(|ui| {
                    ui.add_space(ui.available_width() / 2.0 - rw / 2.0);
                    ui.vertical(|ui| {
                        self.panels.iter_mut().take(LEFT_SIDE_PANELS).for_each(|x| {
                            x.render(ctx, ui);
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            ui.label(if self.queue.connected() {
                                egui::RichText::new("connected")
                                    .color(egui::Color32::GREEN)
                                    .strong()
                                    .size(14.0)
                            } else {
                                egui::RichText::new("disconnected")
                                    .color(egui::Color32::RED)
                                    .strong()
                                    .size(14.0)
                            });
                        });
                    });
                    ui.vertical(|ui| {
                        self.panels.iter_mut().skip(LEFT_SIDE_PANELS).for_each(|x| {
                            x.render(ctx, ui);
                        })
                    });
                });
            });
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}
