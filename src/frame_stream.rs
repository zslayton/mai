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

pub struct FrameStream<'a, E, F, C, H> where
  E: 'a + EventedByteStream,
  C: 'a + Codec<F>,
  H: 'a + FrameHandler<E, F, C, H>,
  F: 'a + Send {
  efs: &'a mut EventedFrameStream<E, F>,
  token: Token,
  event_loop: &'a mut EventLoop<FrameEngine<E, F, C, H>>,
  outbox_pool: &'a mut Pool<Outbox<F>>,
}

impl <'a, E, F, C, H> FrameStream<'a, E, F, C, H> where 
  E: 'a + EventedByteStream,
  C: 'a + Codec<F>,
  H: 'a + FrameHandler<E, F, C, H>,
  F: 'a + Send {

  pub fn new <'b> (
      efs: &'b mut EventedFrameStream<E, F>,
      event_loop: &'b mut EventLoop<FrameEngine<E, F, C, H>>,
      outbox_pool: &'b mut Pool<Outbox<F>>,
      token: Token) -> FrameStream<'b, E, F, C, H> {
    FrameStream {
      efs: efs,
      token: token,
      event_loop: event_loop,
      outbox_pool: outbox_pool
    }    
  }

  pub fn token(&self) -> Token {
    self.token
  }

  pub fn send(&mut self, frame: F) {
    let FrameStream {
      ref mut efs,
      token,
      ref mut event_loop,
      ref mut outbox_pool,
    } = *self;
    efs.send(event_loop, token, outbox_pool, frame);
  }
}

// Methods that will only work for TCP Streams
impl <'a, F, C, H> FrameStream<'a, TcpStream, F, C, H> where 
  C: 'a + Codec<F>,
  H: 'a + FrameHandler<TcpStream, F, C, H>,
  F: 'a + Send {

  pub fn peer_addr(&self) -> io::Result<SocketAddr> {
    self.efs.stream.peer_addr()
  }
  
  pub fn local_addr(&self) -> io::Result<SocketAddr> {
    self.efs.stream.local_addr()
  }
}
