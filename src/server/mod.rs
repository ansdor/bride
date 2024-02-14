use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
};

use crate::utils::{GenericResult, UnitResult};

mod queue;

pub use queue::{Response, CommandQueue};

const MESSAGE_DELIMITER: u32 = 0xAAAAAAAA;

#[derive(Default, Debug)]
pub struct ServerHandle {
    pub connected: bool,
    reader: Option<TcpStream>,
    writer: Option<TcpStream>,
}

impl ServerHandle {
    pub fn new() -> Self {
        ServerHandle {
            connected: false,
            reader: None,
            writer: None,
        }
    }

    pub fn connect(&mut self, address: &SocketAddr) -> UnitResult {
        let stream = TcpStream::connect(address)?;
        self.reader = Some(stream.try_clone()?);
        self.writer = Some(stream);
        self.connected = true;
        Ok(())
    }

    pub fn disconnect(self) -> UnitResult {
        if self.connected {
            if let Some(writer) = self.writer {
                writer.shutdown(Shutdown::Both)?;
            }
        }
        Ok(())
    }

    pub fn send(&mut self, message: &str) -> UnitResult {
        if self.connected {
            if let Some(writer) = &mut self.writer {
                let mut message_bytes = vec![];
                message_bytes.write_all(&MESSAGE_DELIMITER.to_le_bytes())?;
                message_bytes.write_all(&(message.len() as u32).to_le_bytes())?;
                message_bytes.write_all(String::from(message).as_bytes())?;
                writer.write_all(&message_bytes)?;
                writer.flush()?;
                Ok(())
            } else {
                Err("Failed to write to server".into())
            }
        } else {
            Err("Not connected to server.".into())
        }
    }

    pub fn receive(&mut self) -> GenericResult<Option<String>> {
        if self.connected {
            if let Some(reader) = &mut self.reader {
                let mut peek_buffer = [0; 8];
                let n = reader.peek(&mut peek_buffer)?;
                if n == peek_buffer.len() {
                    //consume the first 8 bytes from the stream
                    reader.read_exact(&mut peek_buffer)?;
                    let (header, length) = peek_buffer.split_at(std::mem::size_of::<u32>());
                    if u32::from_le_bytes(header.try_into()?) == MESSAGE_DELIMITER {
                        let message_size = u32::from_le_bytes(length.try_into()?) as usize;
                        let mut read_buffer = vec![0; message_size];
                        reader.read_exact(&mut read_buffer)?;
                        let msg = String::from_utf8(read_buffer)?;
                        return Ok(Some(msg));
                    }
                }
                else if n == 0 {
                    //a read of zero bytes means the connection was lost
                    self.connected = false;
                    return Err("Disconnected".into());
                }
                //failed to read a complete message
                Ok(None)
            } else {
                Err("Failed to read from server.".into())
            }
        } else {
            Err("Not connected to server.".into())
        }
    }
}
