use mio::{Evented, EventLoop, Handler, Token};
use mio::util::Slab;
use lifeguard::{Pool, Recycleable, Recycled};

use std::io::{Read, Write};

use std::collections::BTreeMap;
use std::result::Result;

use Buffer;

//TODO: Provide impl for T where T: Evented + Read + Write
pub trait EventedByteStream : Evented + Read + Write {
  fn on_ready(&mut self);
  fn on_readable_bytes(&mut self, &mut Buffer);
  fn on_writable_bytes(&mut self, &Buffer);
  fn on_error(&mut self);
}

pub struct EventedFrameStream<E> where E: EventedByteStream {
  stream: E, 
  buffer: Option<Buffer>,
}

pub struct FrameEngineBuilder<E> where E: EventedByteStream {
  frame_engine: FrameEngine<E>,
  event_loop: EventLoop<FrameEngine<E>>
}

pub struct FrameEngine<E> where E: EventedByteStream {
  // TODO: Add server?
  streams: BTreeMap<Token, EventedFrameStream<E>>,
  buffer_pool: Pool<Buffer>,
  next_token: usize
}

impl <E> FrameEngineBuilder<E> where E: EventedByteStream {

 fn get_next_token(&mut self) {
   let token = self.frame_engine.next_token;
   self.frame_engine.next_token += 1;
   token
 }
    
 pub fn manage(&mut self, evented_byte_stream: E) -> Result<Token, E> {
    let evented_frame_stream = EventedFrameStream {
      stream: evented_byte_stream,
      buffer: None
    };
    let token = self.get_next_token();
    let token = try!(self.event_loop.register(&evented_frame_stream, token.clone()));
    self.streams.insert(token, evented_frame_stream);
  }

  pub fn run(self) {
    let mut event_loop: EventLoop<FrameEngine<E>> = self.event_loop;
    let mut frame_engine : FrameEngine<E> = self.frame_engine;
    for (token, evented_frame_stream) in &self.streams {
      event_loop.register(&evented_frame_stream, token);
    }
    let _ = event_loop.run(self); 
  }
}

impl <E> Handler for FrameEngine<E> where E: Evented {
  type Timeout = ();
  type Message = (); 
}

