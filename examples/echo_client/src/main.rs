extern crate mio;
extern crate mai;
extern crate env_logger;

//use mio::Token;
use mio::tcp::{TcpSocket, TcpStream};
use mai::*;

struct EchoCodec;
struct EchoClient;
struct EchoClientHandler;

impl Protocol for EchoClient {
  type ByteStream = TcpStream;
  type Frame = String;
  type Codec = EchoCodec;
  type Handler = EchoClientHandler;
  type Timeout = usize;
  type Session = ();
}

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
      Err(_error) => return Err(DecodingError::IncompleteFrame)
    };
    Ok(DecodedFrame::new(message, BytesRead(buffer.len())))
  }
}

impl Handler<EchoClient> for EchoClientHandler {
  fn on_ready(&mut self, context: &mut Context<EchoClient>) {
    context.engine().timeout_ms(55, 5_000);
    let mut stream = context.stream();
    println!("Connected to {:?}, issued {:?}",
             stream.peer_addr().unwrap(),
             stream.token());
    let message: String = "Supercalifragilisticexpialidocious!
                    Even though the sound of it
                    Is something quite atrocious
                    If you say it loud enough
                    You\\'ll always sound precocious!
                    Supercalifragilisticexpialidocious!\n".to_owned();
    println!("Sending message...");
    stream.send(message);
  }
  fn on_frame(&mut self, context: &mut Context<EchoClient>, message: String) {
    let stream = context.stream();
    println!("Received a message from {:?}/{:?}: '{}'", 
             stream.peer_addr().unwrap(),
             stream.token(),
             &message.trim_right());
  }
  fn on_timeout(&mut self, timeout: usize) {
    println!("A timeout occurred: #{:?}", timeout);
  }
  fn on_error(&mut self, context: &mut Context<EchoClient>, error: &Error) {
    let stream = context.stream();
    println!("Error. {:?}/{:?}, {:?}",
             stream.peer_addr().unwrap(),
             stream.token(),
             error);
  }
  fn on_closed(&mut self, context: &mut Context<EchoClient>) {
    let stream = context.stream();
    println!("Disconnected from {:?}/{:?}",
             stream.peer_addr().unwrap(),
             stream.token());
  }
}

fn main() {
  env_logger::init().unwrap();
  println!("Connecting to localhost:9999...");
  let address = "0.0.0.0:9999".parse().unwrap();
  let socket = TcpSocket::v4().unwrap();
  let (stream, _complete) = socket.connect(&address).unwrap();
  
  let frame_engine: FrameEngineRemote<EchoClient> = mai::frame_engine(EchoCodec, EchoClientHandler).run();
  frame_engine.manage(stream);
  frame_engine.wait();
}
