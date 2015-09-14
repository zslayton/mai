use std::io;
use std::net::SocketAddr;
use mio::{EventLoop, Token};
use mio::tcp::TcpStream;
use lifeguard::Pool;

use Protocol;
use EventedByteStream;
use FrameEngine;
use Codec;
use Buffer;
use FrameHandler;
use EventedFrameStream;

use evented_frame_stream::Outbox;

pub struct FrameStream<'a, P: ?Sized> where
  P: 'a + Protocol
  {
    event_loop: &'a mut EventLoop<FrameEngine<P>>,
    efs: &'a mut EventedFrameStream<P>,
    token: Token,
    outbox_pool: &'a mut Pool<Outbox<P::Frame>>,
}

impl <'a, P: ?Sized> FrameStream<'a, P> where 
  P: 'a + Protocol
  {
  pub fn new(
      event_loop: &'a mut EventLoop<FrameEngine<P>>,
      efs: &'a mut EventedFrameStream<P>,
      outbox_pool: &'a mut Pool<Outbox<P::Frame>>,
      token: Token) -> FrameStream<'a, P> {
    FrameStream {
      event_loop: event_loop,
      efs: efs,
      token: token,
      outbox_pool: outbox_pool
    }    
  }

  pub fn token(&self) -> Token {
    self.token
  }

  pub fn send(&mut self, frame: P::Frame) {
    let FrameStream {
      ref mut event_loop,
      ref mut efs,
      token,
      ref mut outbox_pool,
    } = *self;
    efs.send(event_loop, token, outbox_pool, frame);
  }

  pub fn timeout_ms(&mut self, timeout: P::Timeout, milliseconds: u64) {
    self.event_loop.timeout_ms(timeout, milliseconds);
  }
}

// Methods that will only work for TCP Streams
impl <'a, P: ?Sized> FrameStream<'a, P> where 
  P: Protocol<ByteStream=TcpStream>
  {
  pub fn peer_addr(&self) -> io::Result<SocketAddr> {
    self.efs.stream.peer_addr()
  }
  
  pub fn local_addr(&self) -> io::Result<SocketAddr> {
    self.efs.stream.local_addr()
  }
}
