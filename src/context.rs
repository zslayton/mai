use std::io;
use std::net::SocketAddr;
use mio::{EventLoop, Token};
use mio::tcp::TcpStream;
use lifeguard::Pool;

use Protocol;
use FrameEngine;
use EventedFrameStream;

use evented_frame_stream::Outbox;

pub struct Context<'a, P: ?Sized> where
  P: 'a + Protocol
  {
    event_loop: &'a mut EventLoop<FrameEngine<P>>,
    efs: &'a mut EventedFrameStream<P>,
    token: Token,
    outbox_pool: &'a mut Pool<Outbox<P::Frame>>,
    application: &'a mut P::Application,
}

pub struct EngineHandle<'handle, 'context: 'handle, P: ?Sized> where
  P: 'context + Protocol
  {
    context: &'handle mut Context<'context, P>,
}

pub struct StreamHandle<'handle, 'context: 'handle, P: ?Sized> where
  P: 'context + Protocol
  {
    context: &'handle mut Context<'context, P>,
}

impl <'handle, 'context: 'handle, P: ?Sized> EngineHandle<'handle, 'context, P> where 
  P: 'context + Protocol
  {
  pub fn application(&mut self) -> &mut P::Application {
    self.context.application
  }

  pub fn timeout_ms(&mut self, timeout: P::Timeout, milliseconds: u64) {
    self.context.event_loop.timeout_ms(timeout, milliseconds);
  }
}

impl <'handle, 'context: 'handle, P: ?Sized> StreamHandle<'handle, 'context, P> where 
  P: 'context + Protocol
  {
  
  pub fn token(&self) -> Token {
    self.context.token
  }

  pub fn send(&mut self, frame: P::Frame) {
    let Context {
      ref mut event_loop,
      ref mut efs,
      token,
      ref mut outbox_pool,
      ..
    } = *self.context;
    efs.send(event_loop, token, outbox_pool, frame);
  }

  pub fn session(&mut self) -> &mut P::Session {
    &mut self.context.efs.session
  }
}

// Methods that will only work for TCP Streams
impl <'handle, 'context: 'handle, P: ?Sized> StreamHandle<'handle, 'context, P> where 
  P: Protocol<ByteStream=TcpStream>
  {
  pub fn peer_addr(&self) -> io::Result<SocketAddr> {
    self.context.efs.stream.peer_addr()
  }
  
  pub fn local_addr(&self) -> io::Result<SocketAddr> {
    self.context.efs.stream.local_addr()
  }
}

impl <'a, P: ?Sized> Context<'a, P> where 
  P: 'a + Protocol
  {
  pub fn new(
      event_loop: &'a mut EventLoop<FrameEngine<P>>,
      efs: &'a mut EventedFrameStream<P>,
      outbox_pool: &'a mut Pool<Outbox<P::Frame>>,
      token: Token,
      application: &'a mut P::Application) -> Context<'a, P> {
    Context {
      event_loop: event_loop,
      efs: efs,
      token: token,
      outbox_pool: outbox_pool,
      application: application
    }
  }

  pub fn engine<'b>(&'b mut self) -> EngineHandle<'b, 'a, P> {
    EngineHandle {
      context: self
    }
  }

  pub fn stream<'b>(&'b mut self) -> StreamHandle<'b, 'a, P> {
    StreamHandle {
      context: self
    }
  }
}

