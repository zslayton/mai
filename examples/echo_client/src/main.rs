extern crate mio;
extern crate mai;
extern crate env_logger;

use mio::tcp::{TcpSocket, TcpStream};

use mai::codec::*;
use mai::FrameHandler;

struct EchoCodec;

impl Codec<String> for EchoCodec {
  fn encode(&mut self, message: &String, buffer: &mut [u8]) -> EncodingResult {
    let bytes = message.as_bytes();
    if bytes.len() > buffer.len() {
      return Err(EncodingError::InsufficientBufferSize);
    }
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == 0 {
          println!("Found null byte at index {}", index);
          buffer[index] = 'Z' as u8;
          continue;
        }
        buffer[index] = byte;
    }
    Ok(BytesWritten(bytes.len()))
  }

  fn decode(&mut self, buffer: &[u8]) -> DecodingResult<String> {
    use std::str;
    let message: String = match str::from_utf8(buffer) {
      Ok(message) => message.to_owned(),
      Err(error) => return Err(DecodingError::IncompleteFrame)
    };
    Ok(DecodedFrame::new(message, BytesRead(buffer.len())))
  }
}

struct EchoFrameHandler;

impl FrameHandler<String> for EchoFrameHandler {
  fn on_frame_received(&mut self, message: String) {
    println!("Received a message: '{}'", &message.trim_right());
  }
  fn on_frame_written(&mut self, message: String) {
    println!("Wrote a message: '{}'", &message.trim_right());
  }
}

fn main() {
  env_logger::init().unwrap();
  println!("Connecting to localhost:9999...");
  let address = "0.0.0.0:9999".parse().unwrap();
  let socket = TcpSocket::v4().unwrap();
  let (stream, _complete) = socket.connect(&address).unwrap();
  
  let mut frame_engine = mai::frame_engine(EchoCodec, EchoFrameHandler);
  let token = frame_engine.manage(stream);
  frame_engine.send(token, "Supercalifragilisticexpialidocious!
                    Even though the sound of it
                    Is something quite atrocious
                    If you say it loud enough
                    \"You'll\" always sound precocious!
                    Supercalifragilisticexpialidocious!\n".to_owned());
/*  frame_engine.send(token, "supercalifragilisticexpialidocious\n".to_owned());
  frame_engine.send(token, "even though the sound of it\n".to_owned());
  frame_engine.send(token, "is something quite atrocious\n".to_owned());
  frame_engine.send(token, "every time you say it you\n".to_owned());
  frame_engine.send(token, "will always sound precocious\n".to_owned());
  frame_engine.send(token, "supercalifragilisticexpialidocious\n".to_owned());
  */
  frame_engine.run();
}
