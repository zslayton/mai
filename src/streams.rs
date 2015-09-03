use mio::{Evented, EventSet, EventLoop, Handler, Token};
use mio::util::Slab;
use lifeguard::{Pool, RcRecycled};

use std::marker::PhantomData;
use std::io::{self, Read, Write, ErrorKind};
use std::borrow::BorrowMut;
use std::collections::VecDeque;

use slab::Index;

use self::StreamState::*;
use Buffer;
use codec::*;

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
    buffer.truncate(0);//TODO: This can't be right.
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
    debug!("Writing to bytestream");
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
          panic!("Unknown error while writing: {:?}", error);
          return Err(error);
        }
      }
    }
    self.flush(); // Necessary?
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
pub struct EventedFrameStream<E, F> where E: EventedByteStream {
  stream: E,
  state: StreamState,
  // TODO: Could be Option<Recycled<'a, Buffer>> with lifetimes
  read_buffer: Option<RcRecycled<Buffer>>,
  write_buffer: Option<RcRecycled<Buffer>>,
  outbox: Option<RcRecycled<VecDeque<F>>>
}

pub trait FrameHandler<F> {
  fn on_frame_received(&mut self, F);
  fn on_frame_written(&mut self, F);
}

pub struct FrameEngineBuilder<E, F, C, H> where 
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<F>
{
  pub frame_engine: FrameEngine<E, F, C, H>, // TODO: make private?
  pub event_loop: EventLoop<FrameEngine<E, F, C, H>>
}

pub enum FrameEngineError {
  NoSuchToken
}

pub struct FrameEngine<E, F, C, H> where 
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<F>
{
  // TODO: Add server?
  //pub streams: RefCell<Slab<EventedFrameStream<E,F>>>, //TODO: Make this a BTreeMap?
  pub streams: Slab<EventedFrameStream<E,F>>, //TODO: Make this a BTreeMap?
  pub buffer_pool: Pool<Buffer>,
  pub outbox_pool: Pool<VecDeque<F>>,
  pub next_token: usize,
  pub codec: C,
  pub frame_handler: H,
  frame_type: PhantomData<F>
}

impl <E, F, C, H> FrameEngineBuilder<E, F, C, H> where
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<F>
{

 // TODO: This should return a Result
 pub fn manage(&mut self, evented_byte_stream: E) -> Token {
    let evented_frame_stream = EventedFrameStream {
      stream: evented_byte_stream,
      state: StreamState::NotReady,
      read_buffer: None,
      write_buffer: None,
      outbox: None
    };
    //let mut streams = self.frame_engine.streams.borrow_mut();
    let streams = &mut self.frame_engine.streams;
    // Store new stream, get token to use
    let token: Token = match streams.insert(evented_frame_stream) {
      Ok(token) => token,
      _ => panic!("Out of room for tokens!") // Dumb limitation
    };
    let efs: &EventedFrameStream<E,F> = streams.get(token).expect("Missing just-stored EFS");
    //TODO: Handle error?
    let _ = self.event_loop.register(&efs.stream, token).unwrap();
    token
  }

  // TODO: Frame convenience traits, like Frame::From
  pub fn send(&mut self, token: Token, frame: F) -> Result<(), FrameEngineError> { //TODO: Formal error type
    self.frame_engine.send(token, frame)
  }

  pub fn run(self) {
    let mut event_loop: EventLoop<FrameEngine<E, F, C, H>> = self.event_loop;
    let mut frame_engine : FrameEngine<E, F, C, H> = self.frame_engine;
    let _ = event_loop.run(&mut frame_engine); 
  }
}

impl <E, F, C, H> Handler for FrameEngine<E, F, C, H> where
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<F>
{
  type Timeout = ();
  type Message = ();

  fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, event_set: EventSet) {
    trace!("{:?} ready, EventSet={:?}", token, event_set);
    //let mut efs = self.streams.borrow_mut().remove(token).unwrap();
    let mut efs = self.streams.remove(token).expect("Missing token!");
    // Get temporary buffers ready in case the specified stream doesn't already have one
    // TODO: Only get these as needed if you can think around the borrow checker
    let mut updated_state: StreamState;
    //let efs: &mut EventedFrameStream<E,F> = streams.get_mut(token).unwrap();

    if event_set.is_writable() {
      self.write(token, &mut efs);
    }

    if event_set.is_readable() {
      self.read(token, &mut efs);
    }

    if event_set.is_hup() {
      FrameEngine::hup(event_loop, token, &mut efs);
    }

    if event_set.is_error() {
      FrameEngine::error(event_loop, token, &mut efs);
    }

    updated_state = efs.state;

    if let Done = updated_state {
      debug!("Shutting down stream. {:?}", token);
      //streams.remove(token); //TODO: Solve token removal, unregistration
      event_loop.shutdown();
    }
    //TODO: Do this without remove/replace
    let new_token = self.streams.borrow_mut().insert(efs).ok().expect("Couldn't re-insert efs"); 
    assert_eq!(token, new_token);
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

//TODO: Configurable
const BUFFER_POOL_SIZE: usize = 16;
const OUTBOX_POOL_SIZE: usize = 16;
const MAX_STREAMS: usize = 1_024;

impl <E, F, C, H> FrameEngine<E, F, C, H> where
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<F>
{
  pub fn new(codec: C, frame_handler: H) -> FrameEngine<E, F, C, H> {
    FrameEngine {
      //streams: RefCell::new(Slab::new(MAX_STREAMS)),
      streams: Slab::new(MAX_STREAMS),
      buffer_pool: Pool::with_size_and_max(BUFFER_POOL_SIZE, BUFFER_POOL_SIZE),
      outbox_pool: Pool::with_size_and_max(OUTBOX_POOL_SIZE, OUTBOX_POOL_SIZE),
      next_token: 0,
      codec: codec,
      frame_handler: frame_handler,
      frame_type: PhantomData
    }
  }

  pub fn send(&mut self, token: Token, frame: F) -> Result<(), FrameEngineError> { //TODO: Formal error type
    // Get the appropriate efs
    // TODO: Break this into its own helper function
    let mut backup_outbox = self.outbox_pool.new_rc(); //TODO: Avoidable
    let efs: &mut EventedFrameStream<E,F> = match self.streams.get_mut(token) {
      Some(efs) => efs,
      None => return Err(FrameEngineError::NoSuchToken)
    };
    
    let mut outbox = efs.outbox
        .take()
        .or(Some(backup_outbox))
        .expect("Couldn't get an outbox!");

    outbox.push_back(frame);
    efs.outbox = Some(outbox);
    Ok(())
  }

  fn read( &mut self,
           token: Token, 
           efs: &mut EventedFrameStream<E,F>
          ) {
    match efs.state {
      NotReady => { // The 'readable' event signals a fully connected socket
        debug!("Stream for {:?} is Ready.", token);
        efs.state = Ready;
        efs.stream.on_ready();
        self.read_frames(token, efs);
        // No need to read; we're using level triggering so this
        // will be called again with the state modified
      },
      Ready => {
        // TODO: See if it already has a buffer
        debug!("Stream for {:?} can be read.", token);
        self.read_frames(token, efs); 
      },
      Done => {
        debug!("'Readable' event on 'Done' stream. {:?}", token); 
      }
    }
  }

  // Acquire a buffer, read as many bytes as possible into it without blocking,
  // and then parse as many frames as possible from those bytes, invoking the
  // necessary callback each time.
  fn read_frames(&mut self, token: Token, efs: &mut EventedFrameStream<E,F>) {
    let mut read_buffer = efs.read_buffer
      .take()
      .or_else(|| Some(self.buffer_pool.new_rc()))
      .expect("Couldn't get a read buffer!");
    let mut frame_count : usize = 0;
    match efs.stream.on_readable(&mut read_buffer) {
      Ok(bytes_read) => {
        debug!("{} bytes read.", bytes_read);
        let mut start_of_remaining: usize = 0;
        loop {
          let filled = &read_buffer.bytes()[start_of_remaining..bytes_read];
          if filled.len() == 0 {
            debug!("Received 0 bytes, frame reader yielding");
            break;
          }
          if start_of_remaining == bytes_read {
            debug!("Read all available bytes.");
            break;
          }
          debug!("Attempting to decode range [{}..{}] of buffer", start_of_remaining, bytes_read);
          let frame = match self.codec.decode(filled) {
            Ok(decoded_frame) => {
              let BytesRead(size_in_bytes) = decoded_frame.bytes_read;
              debug!("Got a decoded frame {} bytes long", size_in_bytes);
              start_of_remaining += size_in_bytes;
              decoded_frame.frame
            },
            Err(DecodingError::IncompleteFrame) => break,
            Err(error) => panic!("Unexpected error while reading from {:?}", token) //TODO: Handle gracefully
          };
          frame_count += 1;
          debug!("Calling frame handler");
          self.frame_handler.on_frame_received(frame);
        }
      },
      Err(error) => {
        // bad
        debug!("Encountered an error: {:?}", error);
      }
    };
    debug!("Read and processed {} frames.", frame_count);
  }

  fn write( &mut self,
           token: Token, 
           efs: &mut EventedFrameStream<E,F>) {
    match efs.state {
      NotReady => { // The 'writable' event signals a fully connected socket
        debug!("Stream for {:?} is Ready.", token);
        efs.state = Ready;
        efs.stream.on_ready();
        self.write_frames(token, efs);
      },
      Ready => {
        trace!("Stream for {:?} can be written to.", token);
        self.write_frames(token, efs);
      },
      Done => {
        debug!("'Writable' event on 'Done' stream. {:?}", token); 
      }
    }
  }

  // Acquire a buffer, serialize as many frames as will fit into it,
  // and then write as much of that buffer into the stream as possible without
  // blocking.
  fn write_frames(&mut self, token: Token, efs: &mut EventedFrameStream<E,F>) {
    if efs.outbox.is_none() && efs.write_buffer.is_none() {
  //debug!("No writes pending for {:?}, yielding", token);
      return;
    };

    // Either continue using this efs' write buffer
    // or retrieve a new one from the pool
    let mut write_buffer = efs.write_buffer
      .take()
      .or_else(|| Some(self.buffer_pool.new_rc()))
      .expect("Couldn't get a write buffer!");

    // Similarly, get a frame queue to work with
    let mut outbox = efs.outbox
        .take()
        .or_else(|| Some(self.outbox_pool.new_rc()))
        .expect("Couldn't get an outbox!");

    // Serialize frames into the buffer until it's full
    // or we've run out of frames
    let mut frame_count: usize = 0;
    let mut total_bytes_written: usize = 0;
    while outbox.len() > 0 {
      let frame: F = outbox.pop_front().expect("Outbox has len>0 but no messages.");
      let buffer_to_fill = write_buffer.remaining();
      match self.codec.encode(&frame, buffer_to_fill) {
        Ok(BytesWritten(bytes_written)) => {
          debug!("Serialized frame as {} bytes", bytes_written);
          frame_count += 1;
          total_bytes_written += bytes_written;
        },
        Err(EncodingError::InsufficientBufferSize) => {
          //TODO: Check whether the buffer is empty.
          // If it is, this frame will never be writable and the efs should probably die.
          debug!("Could not serialize frame: not enough buffer space.");
          // We need to try serializing again later
          outbox.push_front(frame);
          break;
        },
        Err(error) => {
          outbox.push_front(frame);
          panic!("Could not serialize frame: {:?}.", error);
          // TODO: Call an error handler on the efs, kill it instead of panicking
        }
      };
    }
    debug!("Serialized {} frames into the buffer.", frame_count);
    let buffer_starting_size: usize = write_buffer.len();
    let buffer_current_size: usize = buffer_starting_size + total_bytes_written;
    debug!("Write buffer grew from {} bytes to {} bytes.", buffer_starting_size, buffer_current_size);
    write_buffer.set_size(buffer_current_size);

    // If there are still frames left in the outbox,
    // pin it to this efs. Otherwise it will return
    // to the Pool at the end of the method call.
    if outbox.len() > 0 {
      efs.outbox = Some(outbox);
    }

    match efs.stream.on_writable(&mut write_buffer) {
      Ok(bytes_written) => {
        debug!("{} bytes written to the bytestream.", bytes_written);
      },
      Err(error) => {
        // bad
        panic!("Encountered an unexpected error while writing to the bytestream: {:?}", error);
      }
    };
  }

  fn hup(event_loop: &mut EventLoop<Self>, 
           token: Token, 
           efs: &mut EventedFrameStream<E,F>
         ) {
      debug!("'Hup' event on '{:?}' stream. {:?}", efs.state, token); 
      efs.state = Done;
      event_loop.shutdown();
  }

  fn error(event_loop: &mut EventLoop<Self>, 
           token: Token, 
           efs: &mut EventedFrameStream<E,F>
          ) {
      debug!("'Error' event on '{:?}' stream. {:?}", efs.state, token); 
      efs.state = Done;
  }
}
