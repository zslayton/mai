use lifeguard::{RcRecycled, Pool};
use mio::{EventLoop, EventSet, Token, PollOpt};
use mio::Sender as MioSender;

use std::collections::VecDeque;
use std::io;

use Codec;
use Command;
use Context;
use Handler;
use Error;
use Error::*;
use StreamState::*;
use Protocol;
use Buffer;
use ProtocolEngine;

#[derive(Debug,Clone,Copy)]
pub enum StreamState {
  NotReady,
  Ready,
  Done
}

pub type Outbox<F> = VecDeque<F>;

#[derive(Debug)]
pub struct EventedFrameStream<P: ?Sized> where P: Protocol {
  pub stream: P::ByteStream,
  pub state: StreamState,
  pub codec: P::Codec,
  pub read_buffer: Option<RcRecycled<Buffer>>,
  pub write_buffer: Option<RcRecycled<Buffer>>,
  pub outbox: Option<RcRecycled<Outbox<P::Frame>>>,
}

impl <P: ?Sized> EventedFrameStream<P> where P: Protocol {
  pub fn new(ebs: P::ByteStream) -> EventedFrameStream<P> {
    EventedFrameStream {
      stream: ebs,
      state: StreamState::NotReady,
      codec: P::Codec::new(),
      read_buffer: None,
      write_buffer: None,
      outbox: None,
    }
  }

  pub fn release_empty_buffers(&mut self) {
    let mut drop_read_buffer = false;
    if self.read_buffer.is_some() {
      let read_buffer = self.read_buffer.as_mut().unwrap();
      if read_buffer.len() == 0 {
        drop_read_buffer = true;
      }
    }

    let mut drop_write_buffer = false;
    if self.write_buffer.is_some() {
      let write_buffer = self.write_buffer.as_mut().unwrap();
      if write_buffer.len() == 0 {
        drop_write_buffer = true;
      }
    }

    let mut drop_outbox = false;
    if self.outbox.is_some() {
      let outbox = self.outbox.as_mut().unwrap();
      if outbox.len() == 0 {
        drop_outbox = true;
      }
    }

    if drop_read_buffer {
      debug!("Read buffer is empty. Releasing it to the pool.");
      self.read_buffer = None;
    }
    if drop_write_buffer {
      debug!("Write buffer is empty. Releasing it to the pool.");
      self.write_buffer = None;
    }
    if drop_outbox {
      debug!("Outbox is empty. Releasing it to the pool.");
      self.write_buffer = None;
    }
  }

  pub fn has_bytes_to_write(&self) -> bool {
    // Has bytes in the outbound buffer waiting to be written...
    (!self.write_buffer.is_none() && self.write_buffer.as_ref().unwrap().len() > 0)
    // or frames waiting to be serialized and written
        || (!self.outbox.is_none() && self.outbox.as_ref().unwrap().len() > 0)
  }

  pub fn reading_toolset(&mut self, buffer_pool: &mut Pool<Buffer>) -> (&mut P::Codec, &mut P::ByteStream, &mut Buffer) {
    if self.read_buffer.is_none() {
      debug!("Getting a read_buffer from the pool.");
      self.read_buffer = Some(buffer_pool.new_rc());
    }
    let EventedFrameStream {
      ref mut stream,
      ref mut read_buffer,
      ref mut codec,
      ..
    } = *self;
    (codec, stream, read_buffer.as_mut().unwrap())
  }

  // TODO: Split into encoding_toolset and writing_toolset?
  // Currently an outbox will be allocated if absent, which is silly
  pub fn writing_toolset(&mut self, buffer_pool: &mut Pool<Buffer>, outbox_pool: &mut Pool<Outbox<P::Frame>>) -> (&mut P::Codec, &mut P::ByteStream, &mut Buffer, &mut Outbox<P::Frame>) {
    if self.write_buffer.is_none() {
      debug!("Getting a write_buffer from the pool.");
      self.write_buffer = Some(buffer_pool.new_rc());
    }
    if self.outbox.is_none() {
      debug!("Getting an outbox from the pool.");
      self.outbox = Some(outbox_pool.new_rc());
    }
    let EventedFrameStream {
      ref mut stream,
      ref mut write_buffer,
      ref mut outbox,
      ref mut codec,
        ..
    } = *self;
    (codec, stream, write_buffer.as_mut().unwrap(), outbox.as_mut().unwrap())
  }

  pub fn read_buffer(&mut self, buffer_pool: &mut Pool<Buffer>) -> &mut Buffer {
    if self.read_buffer.is_none() {
      debug!("Getting a read_buffer from the pool.");
      self.read_buffer = Some(buffer_pool.new_rc());
    }
    self.read_buffer.as_mut().unwrap()
  }

  pub fn write_buffer(&mut self, buffer_pool: &mut Pool<Buffer>) -> &mut Buffer {
    if self.write_buffer.is_none() {
      debug!("Getting a write_buffer from the pool.");
      self.write_buffer = Some(buffer_pool.new_rc());
    }
    self.write_buffer.as_mut().unwrap()
  }

  pub fn outbox(&mut self, outbox_pool: &mut Pool<Outbox<P::Frame>>) -> &mut Outbox<P::Frame> {
    if self.outbox.is_none() {
      debug!("Getting an outbox from the pool.");
      self.outbox = Some(outbox_pool.new_rc());
    }
    self.outbox.as_mut().unwrap()
  }

  pub fn register_interest_in_writing (
    &self,
    event_loop: &mut EventLoop<ProtocolEngine<P>>,
    token: Token) -> io::Result<()> {
      debug!("Registering interest in writable event.");
      event_loop.reregister(
        &self.stream,
        token,
        EventSet::all(),
        PollOpt::level()
      )
  }

  pub fn deregister_interest_in_writing (
    &self,
    event_loop:
    &mut EventLoop<ProtocolEngine<P>>,
    token: Token) -> io::Result<()> {
      debug!("De-registering interest in writable event.");
      let mut interests = EventSet::all();
      interests.remove(EventSet::writable());
      event_loop.reregister(
        &self.stream,
        token,
        interests,
        PollOpt::level()
      )
  }

  pub fn on_error(
    &mut self,
    event_loop: &mut EventLoop<ProtocolEngine<P>>,
    token: Token,
    session: &mut P::Session,
    outbox_pool: &mut Pool<Outbox<P::Frame>>,
    command_sender: &mut MioSender<Command<P>>,
    handler: &mut P::Handler,
    error: &Error) {
      if let Io(_) = *error {
        debug!("{:?} encountered an i/o error. Setting state to 'Done'.", token);
        self.state = Done;
      } //TODO: Other non-fatal error types should move state to 'ShuttingDown'
      let context = &mut Context::new(event_loop, self, session, outbox_pool, command_sender, token);
      handler.on_error(context, error);
  }

  pub fn send (
    &mut self,
    event_loop: &mut EventLoop<ProtocolEngine<P>>,
    token: Token,
    outbox_pool: &mut Pool<Outbox<P::Frame>>,
    frame: P::Frame) -> io::Result<()> {

    // If we weren't waiting to write before this, register interest
    // in case we had deregistered it previously.
    if !self.has_bytes_to_write() {
      try!(self.register_interest_in_writing(event_loop, token))
    }

    // Get the outbox (from the pool if necessary) and add our frame
    let mut outbox = self.outbox(outbox_pool);
    outbox.push_back(frame);
    debug!("Outbox has {} messages.", outbox.len());
    Ok(())
  }
}
