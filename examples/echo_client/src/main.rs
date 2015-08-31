extern crate mio;
extern crate mai;
extern crate env_logger;

use mio::tcp::{TcpSocket, TcpStream};

fn main() {
  env_logger::init().unwrap();
  println!("Connecting to localhost:9999...");
  let address = "0.0.0.0:9999".parse().unwrap();
  let socket = TcpSocket::v4().unwrap();
  let (stream, _complete) = socket.connect(&address).unwrap();
  
  let mut frame_engine = mai::frame_engine(16);
  frame_engine.manage(stream);
  frame_engine.run();
}
