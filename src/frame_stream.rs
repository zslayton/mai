use std::io;
use std::net::SocketAddr;
use mio::{EventLoop, Token};
use mio::tcp::TcpStream;
use lifeguard::Pool;

use EventedByteStream;
use FrameEngine;
use Codec;
use Buffer;
use FrameHandler;
use EventedFrameStream;

use evented_frame_stream::Outbox;

pub struct FrameStream<'a, E, F,> where
  E: 'a + EventedByteStream,
  F: 'a + Send {
  efs: &'a mut EventedFrameStream<E, F>,
  token: Token,
  outbox_pool: &'a mut Pool<Outbox<F>>,
}

impl <'a, E, F> FrameStream<'a, E, F> where 
  E: 'a + EventedByteStream,
  F: 'a + Send {

  pub fn new <'b> (
      efs: &'b mut EventedFrameStream<E, F>,
      outbox_pool: &'b mut Pool<Outbox<F>>,
      token: Token) -> FrameStream<'b, E, F,> {
    FrameStream {
      efs: efs,
      token: token,
      outbox_pool: outbox_pool
    }    
  }

  pub fn token(&self) -> Token {
    self.token
  }

  pub fn send(&mut self, frame: F) {
    let FrameStream {
      ref mut efs,
      ref mut outbox_pool,
      ..
    } = *self;
    efs.outbox(outbox_pool).push_back(frame);
  }
}

// Methods that will only work for TCP Streams
impl <'a, F> FrameStream<'a, TcpStream, F> where 
  F: 'a + Send {

  pub fn peer_addr(&self) -> io::Result<SocketAddr> {
    self.efs.stream.peer_addr()
  }
  
  pub fn local_addr(&self) -> io::Result<SocketAddr> {
    self.efs.stream.local_addr()
  }
}
