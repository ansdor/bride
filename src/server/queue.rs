use std::{
    collections::VecDeque,
    net::SocketAddr,
};

use super::ServerHandle;
use crate::utils::UnitResult;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Response {
    Success(String, String),
    Error(String, String),
    Nothing,
}

impl Response {
    pub fn decompose(&self) -> (bool, &str, &str, &str) {
        let err = matches!(self, Self::Error(_, _));
        (err, self.identifier(), self.args(), self.result())
    }

    pub fn identifier(&self) -> &str {
        let (c, _) = match self {
            Self::Success(c, r) => (&c[..], &r[..]),
            Self::Error(c, r) => (&c[..], &r[..]),
            Self::Nothing => ("", ""),
        };
        match c.split_once(char::is_whitespace) {
            Some((id, _)) => id,
            None => c,
        }
    }

    pub fn args(&self) -> &str {
        let (c, _) = match self {
            Self::Success(c, r) => (&c[..], &r[..]),
            Self::Error(c, r) => (&c[..], &r[..]),
            Self::Nothing => ("", ""),
        };
        c.split_once(char::is_whitespace).unwrap_or_default().1
    }

    pub fn result(&self) -> &str {
        match self {
            Self::Success(_, r) => &r[..],
            Self::Error(_, r) => &r[..],
            Self::Nothing => "",
        }
    }
}

#[derive(Debug)]
struct Command {
    command: String,
    args: Vec<String>,
}

impl From<&str> for Command {
    fn from(value: &str) -> Self { Command::from(String::from(value)) }
}

impl From<Command> for String {
    fn from(value: Command) -> Self {
        let mut s = String::new();
        s.push_str(&value.command);
        value.args.iter().for_each(|x| {
            s.push(' ');
            s.push_str(x);
        });
        s
    }
}

impl From<String> for Command {
    fn from(value: String) -> Self {
        let parts: Vec<&str> = value.split(' ').collect();
        let command = String::from(parts[0]);
        let args = parts.into_iter().skip(1).map(String::from).collect();
        Command { command, args }
    }
}

#[derive(Debug)]
pub enum ServerState {
    Disconnected,
    Idle,
    Ready,
    Prompt,
    Paused,
    Finished,
}

impl Default for ServerState {
    fn default() -> Self { Self::Disconnected }
}

#[derive(Default)]
pub struct CommandQueue {
    server_state: ServerState,
    server: Option<ServerHandle>,
    commands: VecDeque<Command>,
    responses: VecDeque<Response>,
}

impl CommandQueue {
    pub fn new() -> Self { Default::default() }

    pub fn connect(&mut self, address: &SocketAddr) -> UnitResult {
        let r = match &mut self.server {
            Some(srv) if srv.connected => Err("Already connected".into()),
            Some(srv) => srv.connect(address),
            None => {
                let mut server = ServerHandle::new();
                server.connect(address)?;
                self.server = Some(server);
                Ok(())
            }
        };
        if let Some(s) = &self.server {
            if s.connected {
                self.server_state = ServerState::Idle;
            }
        }
        r
    }

    pub fn disconnect(self) -> UnitResult {
        if let Some(s) = self.server {
            s.disconnect()?;
        }
        Ok(())
    }

    pub fn send(&mut self, command: &str) {
        if !matches!(self.server_state, ServerState::Paused) {
            self.commands.push_back(Command::from(command));
        }
    }

    pub fn receive(&mut self) -> Option<Response> { self.responses.pop_front() }

    fn update_server_state(message: &str) -> Option<ServerState> {
        match message {
            x if x.starts_with("<READY>") => Some(ServerState::Ready),
            x if x.starts_with("<PAUSE>") => Some(ServerState::Paused),
            x if x.starts_with("<EXIT>") => Some(ServerState::Finished),
            x if x.starts_with("monster>") => Some(ServerState::Prompt),
            _ => None,
        }
    }

    pub fn connected(&self) -> bool {
        match &self.server {
            Some(s) => s.connected,
            None => false,
        }
    }

    pub fn paused(&self) -> bool { matches!(self.server_state, ServerState::Paused) }

    pub fn prompt(&self) -> bool { matches!(&self.server_state, ServerState::Prompt) }

    pub fn finished(&self) -> bool { matches!(self.server_state, ServerState::Finished) }

    pub fn update(&mut self) -> UnitResult {
        if let Some(server) = &mut self.server {
            server.update()?;
            if server.connected {
                while let Some(response) = server.receive() {
                    let analysis = Self::analyze_response(&response);
                    if matches!(analysis, Response::Nothing) {
                        if let Some(state) = Self::update_server_state(&response) {
                            self.server_state = state;
                        } else {
                            return Err(
                                format!("Unrecognized message from server: {}", response).into()
                            );
                        }
                    } else {
                        self.responses.push_back(analysis);
                    }
                }
                while let Some(command) = self.commands.pop_front() {
                    server.send(&String::from(command))?;
                }
            }
        }
        Ok(())
    }

    fn analyze_response(response: &str) -> Response {
        const OK_SIGNAL: &str = "<OK>";
        const ERROR_SIGNAL: &str = "<ERROR>";
        const SEPARATOR: &str = "::";

        let first = match response.lines().map(str::trim).take(1).next() {
            Some(x) => x,
            None => {
                return Response::Nothing;
            }
        };
        let mut rest = String::new();
        let mut lines = response.lines().map(str::trim).skip(1).peekable();
        while let Some(x) = lines.next() {
            rest.push_str(x);
            if lines.peek().is_some() {
                rest.push('\n');
            }
        }
        let header = first.split(SEPARATOR).map(str::trim).collect::<Vec<_>>();
        if header.len() != 2 {
            return Response::Nothing;
        }
        match header[1] {
            x if x == OK_SIGNAL => Response::Success(String::from(header[0]), rest),
            x if x == ERROR_SIGNAL => Response::Error(String::from(header[0]), rest),
            _ => Response::Nothing,
        }
    }
}
