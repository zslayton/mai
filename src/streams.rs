use mio::{Evented, EventSet, EventLoop, Handler, Token};
use mio::util::Slab;
use lifeguard::{Pool, RcRecycled};

use std::io::{Read, Write};

use slab::Index;

use Buffer;

//TODO: Provide impl for T where T: Evented + Read + Write
pub trait EventedByteStream : Evented + Read + Write {
  fn on_ready(&mut self);
  fn on_readable_bytes(&mut self, &mut Buffer);
  fn on_writable_bytes(&mut self, &Buffer);
  fn on_error(&mut self);
}

impl <T> EventedByteStream for T where T: Evented + Read + Write {
  fn on_ready(&mut self) {
    debug!("Impl ready");
  }

  fn on_readable_bytes(&mut self, buffer: &mut Buffer) {
    debug!("Impl ready");
  }

  fn on_writable_bytes(&mut self, buffer: &Buffer) {
    debug!("Impl ready");
  }

  fn on_error(&mut self) {
    debug!("Impl ready");
  }
}

#[derive(Debug)]
pub struct EventedFrameStream<E> where E: EventedByteStream {
  stream: E, 
  // TODO: Could be Option<Recycled<'a, Buffer>> with lifetimes
  buffer: Option<RcRecycled<Buffer>>,
}

pub struct FrameEngineBuilder<E> where E: EventedByteStream {
  pub frame_engine: FrameEngine<E>, // TODO: make private?
  pub event_loop: EventLoop<FrameEngine<E>>
}

pub struct FrameEngine<E> where E: EventedByteStream {
  // TODO: Add server?
  pub streams: Slab<EventedFrameStream<E>>,
  pub buffer_pool: Pool<Buffer>,
  pub next_token: usize
}

impl <E> FrameEngineBuilder<E> where E: EventedByteStream {

 fn get_next_token(&mut self) -> Token {
   let token = self.frame_engine.next_token;
   self.frame_engine.next_token += 1;
   Index::from_usize(token)
 }
    
 // TODO: This should return a Result
 pub fn manage(&mut self, evented_byte_stream: E) {
    let evented_frame_stream = EventedFrameStream {
      stream: evented_byte_stream,
      buffer: None
    };
    // Store new stream, get token to use
    let token = match self.frame_engine.streams.insert(evented_frame_stream) {
      Ok(token) => token,
      _ => return
    };
    let efs_ref: &EventedFrameStream<E> = self.frame_engine.streams.get(token).unwrap();
    //TODO: Handle error?
    let _ = self.event_loop.register(&efs_ref.stream, token).unwrap();
  }

  pub fn run(self) {
    let mut event_loop: EventLoop<FrameEngine<E>> = self.event_loop;
    let mut frame_engine : FrameEngine<E> = self.frame_engine;
    let _ = event_loop.run(&mut frame_engine); 
  }
}

impl <E> Handler for FrameEngine<E> where E: Evented {
  type Timeout = ();
  type Message = ();

  fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, event_set: EventSet) {
    debug!("Token={:?} ready, EventSet={:?}", token, event_set);
  }

  fn notify(&mut self, event_loop: &mut EventLoop<Self>, message: Self::Message) {
    debug!("Notify called, Message={:?}", message); 
  }

  fn timeout(&mut self, event_loop: &mut EventLoop<Self>, timeout: Self::Timeout) {
    debug!("Timeout called, Timeout={:?}", timeout);
  }

  fn interrupted(&mut self, event_loop: &mut EventLoop<Self>) {
    debug!("Interrupted");
  }
}

