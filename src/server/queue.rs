use std::{collections::VecDeque, net::SocketAddr};

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
pub enum ClientState {
    Disconnected,
    WaitingForResponse,
    WaitingForRetrieval,
    Processing,
    Idle,
}

impl Default for ClientState {
    fn default() -> Self { Self::Disconnected }
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
    commands: VecDeque<String>,
    waiting: Option<String>,
    response: Option<Response>,
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
            self.commands.push_back(String::from(command.trim()));
        }
    }

    pub fn receive(&mut self) -> Option<Response> { self.response.take() }

    pub fn internal_state(&self) -> ClientState {
        if !self.connected() {
            ClientState::Disconnected
        } else if self.waiting.is_some() && self.response.is_none() {
            ClientState::WaitingForResponse
        } else if self.waiting.is_none() && self.response.is_some() {
            ClientState::WaitingForRetrieval
        } else if !self.commands.is_empty() {
            ClientState::Processing
        } else {
            ClientState::Idle
        }
    }

    fn update_server_state(&mut self, message: &str) -> UnitResult {
        self.server_state = match message {
            x if x.starts_with("<READY>") => ServerState::Ready,
            x if x.starts_with("<PAUSE>") => ServerState::Paused,
            x if x.starts_with("<EXIT>") => ServerState::Finished,
            x if x.starts_with("monster>") => ServerState::Prompt,
            _ => {
                return Err("Unrecognized".into());
            }
        };
        Ok(())
    }

    pub fn connected(&self) -> bool {
        match &self.server {
            Some(s) => s.connected,
            None => false,
        }
    }

    pub fn paused(&self) -> bool { matches!(self.server_state, ServerState::Paused) }

    pub fn prompt(&self) -> bool {
        matches!(
            (&self.server_state, self.internal_state()),
            (ServerState::Prompt, ClientState::Idle)
        )
    }

    pub fn finished(&self) -> bool { matches!(self.server_state, ServerState::Finished) }

    pub fn update(&mut self) -> UnitResult {
        if let Some(server) = &mut self.server {
            server.update()?;
        }
        match self.internal_state() {
            //if it's disconnected, there's nothing to do
            ClientState::Disconnected => Ok(()),
            //if it's waiting for response, try to get it
            ClientState::WaitingForResponse => {
                //the server is guaranteed to exist and be connected
                if let Some(server) = &mut self.server {
                    if let Some(response) = server.receive() {
                        let analysis = Self::analyze_response(&response);
                        if matches!(analysis, Response::Nothing) {
                            if self.update_server_state(&response).is_err() {
                                return Err(format!(
                                    "Unrecognized message from server: {}",
                                    response
                                )
                                .into());
                            }
                        } else {
                            self.response.replace(analysis);
                            self.waiting.take();
                        }
                    }
                }
                Ok(())
            }
            //the response to the last command is on queue,
            //waiting to be picked up from somewhere else
            ClientState::WaitingForRetrieval | ClientState::Idle => {
                if !self.prompt() {
                    if let Some(server) = &mut self.server {
                        if let Some(msg) = server.receive() {
                            if self.update_server_state(&msg).is_err() {
                                return Err(
                                    format!("Unrecognized message from server: {}", msg).into()
                                );
                            }
                        }
                    }
                }
                Ok(())
            }
            //there are commands ready to be sent to the server
            ClientState::Processing => {
                if let Some(server) = &mut self.server {
                    let command = self.commands.pop_front().unwrap();
                    server.send(&command)?;
                    self.waiting.replace(command);
                }
                Ok(())
            }
        }
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
