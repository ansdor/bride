use std::{env, process};

use egui::ViewportBuilder;
use screen::Screen;
use shell::Shell;
use utils::UnitResult;

mod screen;
mod server;
mod shell;
mod utils;

const DEFAULT_PORT: u16 = 33760;
const PROGRAM_NAME: &str = env!("CARGO_PKG_NAME");
const PROGRAM_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> eframe::Result<()> {
    let (shell, port) = parse_command_line_args(env::args().collect());
    if shell {
        if let Err(msg) = run_shell_mode(port) {
            eprintln!("[ERROR] {msg}");
            process::exit(1);
        }
        Ok(())
    } else {
        let options = eframe::NativeOptions {
            viewport: ViewportBuilder::default()
                .with_title(format!("{} [v{}]", PROGRAM_NAME, PROGRAM_VERSION))
                .with_inner_size([1280.0, 720.0])
                .with_resizable(true),
            ..Default::default()
        };
        eframe::run_native(
            "bride",
            options,
            Box::new(move |cc| {
                egui_extras::install_image_loaders(&cc.egui_ctx);
                cc.egui_ctx.set_visuals(egui::Visuals::dark());
                Box::new(Screen::new(port, cc))
            }),
        )
    }
}

fn parse_command_line_args(args: Vec<String>) -> (bool, u16) {
    let port = match args.iter().position(|x| x == "-p" || x == "--port") {
        Some(n) => {
            if let Some(port) = args.get(n+1) {
                if let Ok(port) = port.parse::<u16>() {
                    port
                } else {
                    DEFAULT_PORT
                }
            } else {
                DEFAULT_PORT
            }
        },
        None => DEFAULT_PORT
    };
    let shell = args.iter().filter(|x| *x == "-s" || *x == "--shell").count() > 0;
    (shell, port)
}

fn run_shell_mode(port: u16) -> UnitResult {
    let mut shell = Shell::new(port);
    shell.interactive_loop()?;
    shell.shutdown()?;
    Ok(())
}
