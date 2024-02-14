use std::net::SocketAddr;

use crate::{
    server::CommandQueue,
    utils::UnitResult,
};

pub struct Shell {
    queue: CommandQueue,
}

impl Shell {
    pub fn new(port: u16) -> Self {
        let mut queue = CommandQueue::new();
        let _ = queue.connect(&SocketAddr::from(([127, 0, 0, 1], port)));
        Shell { queue }
    }

    pub fn interactive_loop(&mut self) -> UnitResult {
        let mut rl = rustyline::DefaultEditor::new()?;
        'interact: loop {
            self.queue.update()?;
            if let Some(msg) = self.queue.receive() {
                let (err, _, _, resp) = msg.decompose();
                let status = if err { "ERROR" } else { "OK" };
                println!("[{}] {}", status, resp);
            }
            if self.queue.finished() {
                break 'interact;
            }
            if self.queue.prompt() {
                let read = rl.readline("bride> ");
                if let Ok(line) = read {
                    rl.add_history_entry(line.as_str())?;
                    self.queue.send(&line);
                } else {
                    break 'interact;
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
