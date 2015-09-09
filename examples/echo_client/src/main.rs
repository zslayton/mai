extern crate mio;
extern crate mai;
extern crate env_logger;

use mio::Token;
use mio::tcp::TcpSocket;

use mai::codec::*;
use mai::{FrameHandler, FrameStream, Error};

struct EchoCodec;

impl Codec<String> for EchoCodec {
  fn encode(&mut self, message: &String, buffer: &mut [u8]) -> EncodingResult {
    let bytes = message.as_bytes();
    if bytes.len() > buffer.len() {
      return Err(EncodingError::InsufficientBuffer);
    }
    for (index, &byte) in bytes.iter().enumerate() {
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

impl FrameHandler<TcpStream, String> for EchoFrameHandler {
  fn on_ready(&mut self, stream: &mut FrameStream) {
    println!("Connected to {:?}, issued {:?}", stream.peer_addr(), stream.token());
  }
  fn on_frame(&mut self, stream: &mut FrameStream, message: String) {
    println!("Received a from {:?}/{:?}: '{}'", stream.peer_addr(), stream.token(), &message.trim_right());
  }
  fn on_error(&mut self, stream: &mut FrameStream, error: Error) {
    println!("Error. {:?}/{:?}, {:?}", stream.peer_addr(), stream.token(), error);
  }
  fn on_closed(&mut self, token: Token) {
    println!("Disconnected from {:?}/{:?}", stream.peer_addr(), stream.token());
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

//frame_engine.send(token, "Supercalifragilisticexpialidocious!
  let message: String = "Supercalifragilisticexpialidocious!
                    Even though the sound of it
                    Is something quite atrocious
                    If you say it loud enough
                    You\\'ll always sound precocious!
                    Supercalifragilisticexpialidocious!\n".to_owned();
  frame_engine.send(token, message);
  let _ = frame_engine.run();
}
