use mio::{Evented, EventSet, EventLoop, Handler, Token};
use mio::util::Slab;
use lifeguard::{Pool, RcRecycled};

use std::io::{self, Read, Write, ErrorKind};

use slab::Index;
use self::StreamState::*;
use Buffer;

//TODO: Provide impl for T where T: Evented + Read + Write
pub trait EventedByteStream : Evented + Read + Write {
  fn on_ready(&mut self);
  fn on_readable(&mut self, &mut Buffer) -> io::Result<usize>;
  fn on_writable(&mut self, &mut Buffer) -> io::Result<usize>;
  fn on_hup(&mut self);
  fn on_error(&mut self);
}

impl <T> EventedByteStream for T where T: Evented + Read + Write {
  fn on_ready(&mut self) {
    debug!("EventedByteStream ready");
  }

  fn on_readable(&mut self, buffer: &mut Buffer) -> io::Result<usize> {
    debug!("EventedByteStream readable");
    buffer.truncate(0);
    let mut total_bytes_read: usize = 0;
    loop {
      buffer.set_size(total_bytes_read);
      let raw_buffer: &mut [u8] = buffer.remaining();
      if raw_buffer.len() == 0 {
        debug!("Buffer is full. Yielding.");
        return Ok(total_bytes_read);
      }
      debug!("Reading from EventedByteStream, {} bytes free", raw_buffer.len());
      match self.read(raw_buffer) {
        Ok(bytes_read) => {
          debug!("Read {} bytes", bytes_read);
          if bytes_read == 0 {
            return Ok(total_bytes_read);
          }
          total_bytes_read += bytes_read;
        },
        Err(error) => {
          if error.kind() == ErrorKind::WouldBlock {
            debug!("Read error WouldBlock, yielding.");
            return Ok(total_bytes_read);
          }
          debug!("A differeng kind of error happened: {:?}", error);
          return Err(error);
        }
      }
    }
  }

  fn on_writable(&mut self, buffer: &mut Buffer) -> io::Result<usize> {
    debug!("EventedByteStream writable");
    let mut total_bytes_written: usize = 0;
    loop {
      let working_buffer: &[u8] = &buffer.bytes()[total_bytes_written..];
      if working_buffer.len() == 0 {
        debug!("Buffer is empty. Yielding.");
        break;
      }
      match self.write(working_buffer) {
        Ok(bytes_written) => {
          debug!("Wrote {} bytes", bytes_written);
          total_bytes_written += bytes_written;
        },
        Err(error) => {
          if error.kind() == ErrorKind::WouldBlock {
            debug!("Write error WouldBlock, yielding.");
            break;
          }
          return Err(error);
        }
      }
    }
    buffer.restack(total_bytes_written);
    return Ok(total_bytes_written);
  }
  
  fn on_hup(&mut self) {
    debug!("EventedByteStream hup");
  }

  fn on_error(&mut self) {
    debug!("EventedByteStream error");
  }
}

#[derive(Debug,Clone,Copy)]
enum StreamState {
  NotReady,
  Ready,
  Done
}

#[derive(Debug)]
pub struct EventedFrameStream<E> where E: EventedByteStream {
  stream: E,
  state: StreamState,
  // TODO: Could be Option<Recycled<'a, Buffer>> with lifetimes
  read_buffer: Option<RcRecycled<Buffer>>,
  write_buffer: Option<RcRecycled<Buffer>>,
}

pub trait FrameHandler {
  fn on_frame_received(&mut self);
  fn on_frame_written(&mut self);
}

pub struct FrameEngineBuilder<E> where E: EventedByteStream {
  pub frame_engine: FrameEngine<E>, // TODO: make private?
  pub event_loop: EventLoop<FrameEngine<E>>
}

pub struct FrameEngine<E> where E: EventedByteStream {
  // TODO: Add server?
  pub streams: Slab<EventedFrameStream<E>>, //TODO: Make this a BTreeMap?
  pub buffer_pool: Pool<Buffer>,
  pub next_token: usize
}

impl <E> FrameEngineBuilder<E> where E: EventedByteStream {

 // TODO: This should return a Result
 pub fn manage(&mut self, evented_byte_stream: E) {
    let evented_frame_stream = EventedFrameStream {
      stream: evented_byte_stream,
      state: StreamState::NotReady,
      read_buffer: None,
      write_buffer: None,
    };
    // Store new stream, get token to use
    let token = match self.frame_engine.streams.insert(evented_frame_stream) {
      Ok(token) => token,
      _ => return
    };
    let efs: &EventedFrameStream<E> = self.frame_engine.streams.get(token).unwrap();
    //TODO: Handle error?
    let _ = self.event_loop.register(&efs.stream, token).unwrap();
  }

  pub fn run(self) {
    let mut event_loop: EventLoop<FrameEngine<E>> = self.event_loop;
    let mut frame_engine : FrameEngine<E> = self.frame_engine;
    let _ = event_loop.run(&mut frame_engine); 
  }
}

impl <E> Handler for FrameEngine<E> where E: EventedByteStream {
  type Timeout = ();
  type Message = ();

  fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, event_set: EventSet) {
    debug!("Token={:?} ready, EventSet={:?}", token, event_set);
    // Get temporary buffers ready in case the specified stream doesn't already have one
    // TODO: Only get these as needed if you can think around the borrow checker
    let tmp_read_buffer = self.buffer_pool.new_rc();
    let tmp_write_buffer = self.buffer_pool.new_rc();
    let mut updated_state: StreamState;
    { // Extra scope to get around lexical borrows
      let efs: &mut EventedFrameStream<E> = self.streams.get_mut(token).unwrap();

      if event_set.is_writable() {
        let mut write_buffer = efs.write_buffer
            .take()
            .or_else(|| Some(tmp_write_buffer))
            .unwrap();
        FrameEngine::write(event_loop, token, efs, &mut write_buffer);
      }

      if event_set.is_readable() {
        let mut read_buffer = efs.read_buffer
            .take()
            .or_else(|| Some(tmp_read_buffer))
            .unwrap();
        FrameEngine::read(event_loop, token, efs, &mut read_buffer);
      }

      if event_set.is_hup() {
        FrameEngine::hup(event_loop, token, efs);
      }

      if event_set.is_error() {
        FrameEngine::error(event_loop, token, efs);
      }

      updated_state = efs.state;
    } // End lexical-scope hack

    if let Done = updated_state {
      debug!("Shutting down stream. Token={:?}", token);
      self.streams.remove(token);
      event_loop.shutdown();
    }
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

impl <E> FrameEngine<E> where E: EventedByteStream {

  fn read( event_loop: &mut EventLoop<Self>, 
           token: Token, 
           efs: &mut EventedFrameStream<E>,
           buffer: &mut Buffer
          ) {
    match efs.state {
      NotReady => { // The 'readable' event signals a fully connected socket
        debug!("Stream for Token={:?} is Ready.", token);
        efs.state = Ready;
        efs.stream.on_ready();
        FrameEngine::read_while_nonblocking(efs, buffer);
        // No need to read; we're using level triggering so this
        // will be called again with the state modified
      },
      Ready => {
        // TODO: See if it already has a buffer
        debug!("Stream for Token={:?} can be read.", token);
        FrameEngine::read_while_nonblocking(efs, buffer);
      },
      Done => {
        debug!("'Readable' event on 'Done' stream. Token={:?}", token); 
      }
    }
  }

  fn read_while_nonblocking(efs: &mut EventedFrameStream<E>,
                           buffer: &mut Buffer) {
    use std::str;
    match efs.stream.on_readable(buffer) {
      Ok(bytes_read) => {
        let message : &str = str::from_utf8(buffer.bytes()).unwrap();
        debug!("Read {} bytes: '{}'", bytes_read, message);
      },
      Err(error) => {
        debug!("Encountered an error: {:?}", error);
      }
    }
  }

  fn write(event_loop: &mut EventLoop<Self>, 
           token: Token, 
           efs: &mut EventedFrameStream<E>,
           buffer: &mut Buffer
          ) {
    match efs.state {
      NotReady => { // The 'writable' event signals a fully connected socket
        debug!("Stream for Token={:?} is Ready.", token);
        efs.state = Ready;
        efs.stream.on_ready();
        // No need to write; we're using level triggering so this
        // will be called again with the state modified
      },
      Ready => {
        // TODO: See if it already has a buffer
        efs.stream.on_writable(buffer);
      },
      Done => {
        debug!("'Writable' event on 'Done' stream. Token={:?}", token); 
      }
    }
  }

  fn hup(event_loop: &mut EventLoop<Self>, 
           token: Token, 
           efs: &mut EventedFrameStream<E>
         ) {
      debug!("'Hup' event on '{:?}' stream. Token={:?}", efs.state, token); 
      efs.state = Done;
      event_loop.shutdown();
  }

  fn error(event_loop: &mut EventLoop<Self>, 
           token: Token, 
           efs: &mut EventedFrameStream<E>
          ) {
      debug!("'Error' event on '{:?}' stream. Token={:?}", efs.state, token); 
      efs.state = Done;
  }
}
