use std::net::SocketAddr;

use crate::{server::CommandQueue, utils::UnitResult};

enum ShellState {
    Read,
    Write,
}

pub struct Shell {
    queue: CommandQueue,
    state: ShellState,
}

impl Shell {
    pub fn new(port: u16) -> Self {
        let mut queue = CommandQueue::new();
        let _ = queue.connect(&SocketAddr::from(([127, 0, 0, 1], port)));
        Shell {
            queue,
            state: ShellState::Read,
        }
    }

    pub fn interactive_loop(&mut self) -> UnitResult {
        let mut rl = rustyline::DefaultEditor::new()?;
        let mut response_received = true;
        'interact: loop {
            self.queue.update()?;
            if self.queue.finished() {
                break 'interact;
            }
            match self.state {
                ShellState::Read => {
                    if let Some(msg) = self.queue.receive() {
                        let (err, _, _, resp) = msg.decompose();
                        let status = if err { "ERROR" } else { "OK" };
                        println!("[{}] {}", status, resp);
                        response_received = true;
                    } else if self.queue.prompt() && response_received {
                        self.state = ShellState::Write;
                    }
                }
                ShellState::Write => {
                    let read = rl.readline("bride> ");
                    if let Ok(line) = read {
                        rl.add_history_entry(line.as_str())?;
                        self.queue.send(&line);
                        self.state = ShellState::Read;
                        response_received = false;
                    } else {
                        break 'interact;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn shutdown(self) -> UnitResult {
        self.queue.disconnect()?;
        Ok(())
    }
}
