extern crate mio;
extern crate mai;
extern crate env_logger;

use std::thread;

use mio::tcp::{TcpSocket, TcpStream, TcpListener};
use mio::{EventLoop, PollOpt, EventSet};
use mai::*;

struct EchoCodec;
struct EchoServer;
struct EchoServerHandler;

#[derive(Default)]
struct EchoSession {
  pub count : usize
}

impl Protocol for EchoServer {
  type ByteStream = TcpStream;
  type Frame = String;
  type Codec = EchoCodec;
  type Handler = EchoServerHandler;
  type Timeout = usize;
  type Session = EchoSession;
}

impl Codec<String> for EchoCodec {

  fn new() -> Self {
    EchoCodec
  }
    
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

impl Handler<EchoServer> for EchoServerHandler {
  fn on_ready(&mut self, context: &mut Context<EchoServer>) {
    let mut stream = context.stream();
    println!("Client connected from {:?}, issued {:?}",
             stream.peer_addr().unwrap(),
             stream.token());
  }
  fn on_frame(&mut self, context: &mut Context<EchoServer>, message: String) {
    let mut stream = context.stream();
    stream.session().count += 1;
    /*println!("Received a message from {:?}/{:?}: '{}'", 
             stream.peer_addr().unwrap(),
             stream.token(),
             &message.trim_right());*/
    stream.send(message);
  }
  fn on_timeout(&mut self, timeout: usize) {
    println!("A timeout occurred: #{:?}", timeout);
  }
  fn on_error(&mut self, context: &mut Context<EchoServer>, error: &Error) {
    let stream = context.stream();
    println!("Error. {:?}/{:?}, {:?}",
             stream.peer_addr().unwrap(),
             stream.token(),
             error);
  }
  fn on_closed(&mut self, context: &mut Context<EchoServer>) {
    let mut stream = context.stream();
    println!("Client {:?}/{:?} disconnected.",
             stream.peer_addr().unwrap(),
             stream.token());
    println!("Client sent/received {} messages.", stream.session().count);
  }
}

struct ClientAcceptor<P>(TcpListener, mio::Sender<Command<P>>) where P: Protocol<ByteStream=TcpStream>;

impl <P> mio::Handler for ClientAcceptor<P> where P: Protocol<ByteStream=TcpStream> {
  type Timeout = ();
  type Message = ();
  fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: mio::Token, _: EventSet) {
    let ClientAcceptor(ref mut listener, ref mut sender) = *self;
    println!("Accepted a client connection.");
    let stream: TcpStream = listener.accept().unwrap().unwrap();
    println!("Sending client to protocol engine.");
    sender.send(Command::Manage(stream));
  }
}

fn main() {
  use std;
  env_logger::init().unwrap();
  println!("Connecting to localhost:9999...");

  let mut protocol_engine: ProtocolEngine<EchoServer> =
      mai::protocol_engine(EchoServerHandler)
        .with(InitialBufferSize(Kilobytes(32)))
        .with(InitialBufferPoolSize(16))
        .with(MaxBufferPoolSize(128))
        .build();

  let address = "127.0.0.1:9999".parse().unwrap();
  let server = TcpListener::bind(&address).unwrap();
  let mut event_loop = EventLoop::new().unwrap();
  event_loop.register(&server, mio::Token(0)).unwrap();
  
  let mut acceptor = ClientAcceptor(server, protocol_engine.channel());

  let handle = thread::spawn(|| {
    println!("Starting protocol engine in the background.");
    let _ = protocol_engine.run();
  });

  println!("Starting acceptor in the foreground.");
  let _ = event_loop.run(&mut acceptor);
  let _ = handle.join();
}
