use std::io;
use std::net::SocketAddr;
use mio::{self, EventLoop, Token};
use mio::tcp::TcpStream;
use mio::NotifyError;
use mio::Sender as MioSender;
use lifeguard::Pool;

use Protocol;
use Command;
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
    command_sender: &'a mut MioSender<Command<P>> 
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
  
  pub fn timeout_ms(&mut self, timeout_token: P::Timeout, milliseconds: u64) -> mio::TimerResult<mio::Timeout> {
    self.context.event_loop.timeout_ms(timeout_token, milliseconds)
  }
  
  pub fn send(&mut self, token: Token, frame: P::Frame) -> Result<(), NotifyError<Command<P>>> {
    let Context {
      ref mut command_sender,
      ..
    } = *self.context;
    command_sender.send(Command::Send(token, frame))
  }

  pub fn broadcast(&mut self, frame: P::Frame) -> Result<(), NotifyError<Command<P>>> {
    let Context {
      ref mut command_sender,
      ..
    } = *self.context;
    command_sender.send(Command::Broadcast(frame))
  }
}

impl <'handle, 'context: 'handle, P: ?Sized> StreamHandle<'handle, 'context, P> where 
  P: 'context + Protocol
  {
  
  pub fn token(&self) -> Token {
    self.context.token
  }

  pub fn send(&mut self, frame: P::Frame) -> io::Result<()> {
    let Context {
      ref mut event_loop,
      ref mut efs,
      token,
      ref mut outbox_pool,
      ..
    } = *self.context;
    efs.send(event_loop, token, outbox_pool, frame)
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
      command_sender: &'a mut MioSender<Command<P>>,
      token: Token) -> Context<'a, P> {
    Context {
      event_loop: event_loop,
      efs: efs,
      token: token,
      outbox_pool: outbox_pool,
      command_sender: command_sender
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

