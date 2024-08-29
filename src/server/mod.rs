use std::{
    collections::VecDeque,
    io::{Read, Write},
    mem,
    net::{Shutdown, SocketAddr, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::utils::UnitResult;

mod queue;

pub use queue::{CommandQueue, Response};

const MESSAGE_DELIMITER: u32 = 0xAAAAAAAA;
const PROMPT_MESSAGE: &str = "monster>";

enum Message {
    Send(String),
    Receive,
    Terminate,
}

#[derive(Default, Debug)]
pub struct ServerHandle {
    pub connected: bool,
    server_thread: Option<JoinHandle<UnitResult>>,
    receiver: Option<Receiver<String>>,
    sender: Option<Sender<Message>>,
    responses: VecDeque<String>,
}

impl ServerHandle {
    pub fn new() -> Self {
        ServerHandle {
            connected: false,
            server_thread: None,
            receiver: None,
            sender: None,
            responses: VecDeque::new(),
        }
    }

    pub fn connect(&mut self, address: &SocketAddr) -> UnitResult {
        let stream = TcpStream::connect(address)?;
        let (mut reader, mut writer) = (stream.try_clone()?, stream.try_clone()?);
        let (cs, cr) = mpsc::channel(); //controller
        let (ms, mr) = mpsc::channel(); //message
        let st = thread::spawn(move || {
            'main: loop {
                match cr.try_recv() {
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("Disconnected from server.".into());
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Ok(msg) => match msg {
                        Message::Terminate => {
                            break;
                        }
                        Message::Send(msg) => {
                            let mut message_bytes = vec![];
                            message_bytes.write_all(&MESSAGE_DELIMITER.to_le_bytes())?;
                            message_bytes.write_all(&(msg.len() as u32).to_le_bytes())?;
                            message_bytes.write_all(msg.as_bytes())?;
                            writer.write_all(&message_bytes)?;
                            writer.flush()?;
                        }
                        Message::Receive => {
                            'recv: loop {
                                thread::sleep(Duration::from_millis(1));
                                let mut peek_buffer = [0; 8];
                                let n = reader.peek(&mut peek_buffer)?;
                                if n == peek_buffer.len() {
                                    //consume the first 8 bytes from the stream
                                    reader.read_exact(&mut peek_buffer)?;
                                    let (header, length) =
                                        peek_buffer.split_at(mem::size_of::<u32>());
                                    if u32::from_le_bytes(header.try_into()?) == MESSAGE_DELIMITER {
                                        let message_size =
                                            u32::from_le_bytes(length.try_into()?) as usize;
                                        let mut read_buffer = vec![0; message_size];
                                        reader.read_exact(&mut read_buffer)?;
                                        let msg = String::from_utf8(read_buffer)?;
                                        let exit = msg.starts_with(PROMPT_MESSAGE);
                                        ms.send(msg)?;
                                        if exit {
                                            break 'recv;
                                        }
                                    }
                                } else if n == 0 {
                                    //a read of zero bytes means the connection was lost
                                    break 'main;
                                }
                            }
                        }
                    },
                }
            }
            writer.shutdown(Shutdown::Both)?;
            Ok(())
        });
        cs.send(Message::Receive)?;
        self.sender = Some(cs);
        self.receiver = Some(mr);
        self.server_thread = Some(st);
        self.connected = true;
        Ok(())
    }

    pub fn disconnect(self) -> UnitResult {
        if self.connected {
            if let (Some(sender), Some(thread)) = (self.sender, self.server_thread) {
                sender.send(Message::Terminate)?;
                match thread.join() {
                    Ok(r) => {
                        println!("Disconnected successfully.");
                        r
                    }
                    Err(_) => Err("Failed to join server thread.".into()),
                }
            } else {
                Err("Failed to access server thread.".into())
            }
        } else {
            Err("Not connected.".into())
        }
    }

    pub fn send(&mut self, message: &str) -> UnitResult {
        if self.connected {
            if let Some(sender) = &mut self.sender {
                let s = sender.send(Message::Send(String::from(message)));
                let r = sender.send(Message::Receive);
                match (s, r) {
                    (Ok(_), Ok(_)) => Ok(()),
                    (Err(e), _) => Err(e.into()),
                    (_, Err(e)) => Err(e.into()),
                }
            } else {
                unreachable!()
            }
        } else {
            Err("Not connected.".into())
        }
    }

    pub fn update(&mut self) -> UnitResult {
        if self.connected {
            if let Some(recv) = &self.receiver {
                match recv.try_recv() {
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("Disconnected from server.".into());
                    }
                    Ok(msg) => {
                        self.responses.push_back(msg);
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    pub fn receive(&mut self) -> Option<String> {
        self.responses.pop_front()
    }
}
