use std::io;
use std::net::SocketAddr;
use mio::{self, EventLoop, Token};
use mio::tcp::{TcpStream, Shutdown};
use mio::NotifyError;
use mio::Sender as MioSender;
use lifeguard::Pool;

use timeout::Timeout;
use Protocol;
use Command;
use ProtocolEngine;
use EventedFrameStream;

use evented_frame_stream::Outbox;

pub struct Context<'a, P: ?Sized> where
  P: 'a + Protocol
  {
    event_loop: &'a mut EventLoop<ProtocolEngine<P>>,
    efs: &'a mut EventedFrameStream<P>,
    session: &'a mut P::Session,
    token: Token,
    outbox_pool: &'a mut Pool<Outbox<P::Frame>>,
    command_sender: &'a mut MioSender<Command<P>>
}

pub struct EngineHandle<'handle, P: ?Sized> where
  P: 'handle + Protocol
  {
      event_loop: &'handle mut EventLoop<ProtocolEngine<P>>,
      command_sender: &'handle mut MioSender<Command<P>>
}

pub struct StreamHandle<'handle, P: 'handle + ?Sized> where
  P: Protocol
  {
      event_loop: &'handle mut EventLoop<ProtocolEngine<P>>,
      command_sender: &'handle mut MioSender<Command<P>>,
      efs: &'handle mut EventedFrameStream<P>,
      token: Token,
      outbox_pool: &'handle mut Pool<Outbox<P::Frame>>
}

impl <'handle, P: ?Sized> EngineHandle<'handle, P> where
  P: 'handle + Protocol
  {

  pub fn new<'a>(
      event_loop: &'a mut EventLoop<ProtocolEngine<P>>,
      command_sender: &'a mut MioSender<Command<P>>) -> EngineHandle<'a, P> {
          EngineHandle {
              event_loop: event_loop,
              command_sender: command_sender
          }
  }

  pub fn timeout_ms(&mut self, timeout_token: P::Timeout, milliseconds: u64) -> mio::TimerResult<mio::Timeout> {
    self.event_loop.timeout_ms(Timeout::global(timeout_token), milliseconds)
  }

  pub fn clear_timeout(&mut self, timeout_token: mio::Timeout) -> bool {
      self.event_loop.clear_timeout(timeout_token)
  }

  pub fn send(&mut self, token: Token, frame: P::Frame) -> Result<(), NotifyError<Command<P>>> {
    self.command_sender.send(Command::Send(token, frame))
  }

  pub fn broadcast(&mut self, frame: P::Frame) -> Result<(), NotifyError<Command<P>>> {
    self.command_sender.send(Command::Broadcast(frame))
  }
}

impl <'handle, P: ?Sized> StreamHandle<'handle, P> where
  P: Protocol
  {

  pub fn token(&self) -> Token {
    self.token
  }

  pub fn send(&mut self, frame: P::Frame) -> io::Result<()> {
    self.efs.send(self.event_loop, self.token, self.outbox_pool, frame)
  }

  pub fn timeout_ms(&mut self, timeout_token: P::Timeout, milliseconds: u64) -> mio::TimerResult<mio::Timeout> {
    self.event_loop.timeout_ms(Timeout::for_stream(self.token, timeout_token), milliseconds)
  }

  pub fn clear_timeout(&mut self, timeout_token: mio::Timeout) -> bool {
      self.event_loop.clear_timeout(timeout_token)
  }
}

// Methods that will only work for TCP Streams
impl <'handle, P: ?Sized> StreamHandle<'handle, P> where
  P: Protocol<ByteStream=TcpStream>
  {
  pub fn peer_addr(&self) -> io::Result<SocketAddr> {
    self.efs.stream.peer_addr()
  }

  pub fn local_addr(&self) -> io::Result<SocketAddr> {
    self.efs.stream.local_addr()
  }

  pub fn shutdown(&mut self) -> io::Result<()> {
    self.efs.stream.shutdown(Shutdown::Both)
  }
}

impl <'a, P: ?Sized> Context<'a, P> where
  P: 'a + Protocol
  {
  pub fn new(
      event_loop: &'a mut EventLoop<ProtocolEngine<P>>,
      efs: &'a mut EventedFrameStream<P>,
      session: &'a mut P::Session,
      outbox_pool: &'a mut Pool<Outbox<P::Frame>>,
      command_sender: &'a mut MioSender<Command<P>>,
      token: Token) -> Context<'a, P> {
    Context {
      event_loop: event_loop,
      efs: efs,
      session: session,
      token: token,
      outbox_pool: outbox_pool,
      command_sender: command_sender
    }
  }

  pub fn engine<'b>(&'b mut self) -> EngineHandle<'b, P> {
      let Context {
        ref mut event_loop,
        ref mut command_sender,
        ..
      } = *self;
      EngineHandle {
        event_loop: event_loop,
        command_sender: command_sender
      }
  }

  pub fn stream_and_session(& mut self) -> (StreamHandle<P>, &mut P::Session) {
      let Context {
          ref mut event_loop,
          ref mut efs,
          ref mut session,
          token,
          ref mut outbox_pool,
          ref mut command_sender,
      } = *self;
      let (stream, session) = (StreamHandle {
        event_loop: event_loop,
        efs: efs,
        token: token,
        outbox_pool: outbox_pool,
        command_sender: command_sender
      }, session);
      (stream, session)
  }

  pub fn stream(&mut self) -> StreamHandle<P> {
      let Context {
          ref mut event_loop,
          ref mut efs,
          token,
          ref mut outbox_pool,
          ref mut command_sender,
          ..
      } = *self;
    StreamHandle {
      event_loop: event_loop,
      efs: efs,
      token: token,
      outbox_pool: outbox_pool,
      command_sender: command_sender
    }
  }

  pub fn session(&mut self) -> &mut P::Session {
    &mut self.session
  }
}
