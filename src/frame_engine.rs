use mio::{Evented, EventSet, PollOpt, EventLoop, Handler, Token};
use mio::Sender as MioSender;
use lifeguard::{Pool, RcRecycled};

use std::{self, thread, mem};
use std::result::Result;
use std::marker::PhantomData;
use std::io::{self, Read, Write, ErrorKind};
use std::borrow::BorrowMut;
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Sender, Receiver};

use slab::Index;

use stream_manager::StreamManager;
use error::Error::{self, Io, Encoding, Decoding};

use FrameEngineRemote;
use FrameStream;
use EventedFrameStream;
use EventedByteStream;
use FrameHandler;
use StreamState;
use StreamState::*;
use Outbox;
use Buffer;
use codec::*;

pub struct FrameEngineBuilder<E, F, C, H> where 
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<E, F>,
  F: Send
{
  pub frame_engine: FrameEngine<E, F, C, H>, // TODO: make private?
  pub event_loop: EventLoop<FrameEngine<E, F, C, H>>,
}

#[derive(Debug)]
pub enum FrameEngineError {
  NoSuchToken
}

pub enum Command<E,F> where E: EventedByteStream {
  Shutdown,
  Manage(E),
  Send(Token, F),
//  SendFrameToList(Vec<Token>, F),
//  BroadcastFrame(F)
}

pub struct FrameEngine<E, F, C, H> where 
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<E, F>
{
  // TODO: Add server
  pub streams: StreamManager<E, F>,
  pub buffer_pool: Pool<Buffer>,
  pub outbox_pool: Pool<Outbox<F>>,
  pub codec: C,
  pub frame_handler: H,
  frame_type: PhantomData<F> //TODO: make a FrameEngine trait so this can be an assoc type?
}

impl <E, F, C, H> FrameEngineBuilder<E, F, C, H> where
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<E, F>,
  F: Send
{

  // TODO: This should return a Result
  // TODO: This should be handled via a Channel
  pub fn manage(&mut self, evented_byte_stream: E) -> Token {
    self.frame_engine.manage(&mut self.event_loop, evented_byte_stream)
  }

  // TODO: Frame convenience traits, like Frame::From
  pub fn send(&mut self, token: Token, frame: F) -> Result<(), FrameEngineError> { //TODO: Formal error type
    self.frame_engine.send(&mut self.event_loop, token, frame)
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
  H: FrameHandler<E, F>,
  F: Send
{
  type Timeout = ();
  type Message = Command<E,F>;

  fn notify(&mut self, event_loop: &mut EventLoop<Self>, command: Command<E,F>) {
    use self::Command::*;
    match command {
      Shutdown => {
        debug!("Received Shutdown command. Shuttind down event loop.");
        event_loop.shutdown();
      },
      Manage(evented_byte_stream) => {
        debug!("Received Manage command. Registering new EventedByteStream.");
        self.manage(event_loop, evented_byte_stream);
      },
      Send(token, frame) => {
        debug!("Received Send command. Sending frome to {:?}", token);
        match self.send(event_loop, token, frame) {
          Err(error) => error!("Failed to send to {:?}: {:?}", token, error),
          _ => {}
        }
      }
    }
  }

  fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, event_set: EventSet) {
    trace!("{:?} ready, EventSet={:?}", token, event_set);

    // Break `self` into mutable references to its components
    let FrameEngine {
      ref mut streams,
      ref mut buffer_pool,
      ref mut outbox_pool,
      ref mut codec,
      ref mut frame_handler,
      ref frame_type
    } = *self;

    let mut stream_is_done = false;
    {
        // Get a reference to the EventedFrameStream we'll be modifying
        let mut efs: &mut EventedFrameStream<E,F> = streams.get_mut(token).expect("Missing token!");

        if event_set.is_writable() {
          FrameEngine::on_writable(event_loop, codec, frame_handler, token, efs, buffer_pool, outbox_pool);
          if efs.has_bytes_to_write() {
            debug!("{:?} is still waiting to write.", token);
          } else {
            debug!("No more to write for {:?}, deregistering interest in writes", token);
            efs.deregister_interest_in_writing(event_loop, token);
          }
        }

        if event_set.is_readable() {
          FrameEngine::on_readable(event_loop, codec, frame_handler, token, efs, buffer_pool, outbox_pool);
        }

        if event_set.is_hup() {
          FrameEngine::hup(event_loop, token, efs);
        }

        if event_set.is_error() {
          FrameEngine::error(event_loop, token, efs);
        }

        // Because `efs` borrows `streams`, we need to process the end
        // of this stream's life in two phases: 1) unregister `efs` from
        // the event_loop and allow it to drop, un-borrowing `streams`
        // and 2) remove the stream itself from the frame_engine
        if let Done = efs.state {
          debug!("Shutting down stream. {:?}", token);
          event_loop.deregister(&efs.stream);
          frame_handler.on_closed(&FrameStream::new(efs, outbox_pool, token));
          stream_is_done = true;
        } else {
          efs.release_empty_buffers();
        }
    } // `streams` is no longer borrowed

    if stream_is_done {
      debug!("Removing stream from FrameEngine");
      let _ = streams.remove(token); // Drop the stream, freeing up pooled resources
    }
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

impl <E, F, C, H> FrameEngine<E, F, C, H> where
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<E, F>,
  F: Send
{
  pub fn new(codec: C, frame_handler: H) -> FrameEngine<E, F, C, H> {
    FrameEngine {
      streams: StreamManager::new(),
      buffer_pool: Pool::with_size_and_max(BUFFER_POOL_SIZE, BUFFER_POOL_SIZE),
      outbox_pool: Pool::with_size_and_max(OUTBOX_POOL_SIZE, OUTBOX_POOL_SIZE),
      codec: codec,
      frame_handler: frame_handler,
      frame_type: PhantomData
    }
  }

  pub fn manage(&mut self, event_loop: &mut EventLoop<FrameEngine<E,F,C,H>>, evented_byte_stream: E) -> Token {
    let evented_frame_stream = EventedFrameStream::new(evented_byte_stream);
    let token = self.streams.insert(evented_frame_stream);
    let efs = self.streams.get_mut(token).expect("Missing just-inserted EventedFrameStream");
    //TODO: Handle error?
    let _ = event_loop.register(&efs.stream, token).unwrap();
    token
  }

  pub fn send(&mut self, event_loop: &mut EventLoop<FrameEngine<E,F,C,H>>, token: Token, frame: F) -> Result<(), FrameEngineError> { //TODO: Formal error type
    let FrameEngine {
      ref mut streams,
      ref mut buffer_pool,
      ref mut outbox_pool,
      ref mut codec,
      ref mut frame_handler,
      ref frame_type
    } = *self;
    // Get the appropriate efs
    // TODO: Break this into its own helper function
    let efs: &mut EventedFrameStream<E,F> = match streams.get_mut(token) {
      Some(efs) => efs,
      None => return Err(FrameEngineError::NoSuchToken)
    };
   
    efs.send(event_loop, token, outbox_pool, frame);
    Ok(())
  }

  fn on_readable(event_loop: &mut EventLoop<FrameEngine<E, F, C, H>>,
          codec: &mut C,
          frame_handler: &mut H,
          token: Token,
          efs: &mut EventedFrameStream<E,F>,
          buffer_pool: &mut Pool<Buffer>,
          outbox_pool: &mut Pool<Outbox<F>>) {
    match efs.state {
      NotReady => { // The 'readable' event signals a fully connected socket
        debug!("Stream for {:?} is now Ready.", token);
        efs.state = Ready;
        frame_handler.on_ready(&mut FrameStream::new(efs, outbox_pool, token));
        FrameEngine::read(event_loop, codec, frame_handler, token, efs, buffer_pool, outbox_pool);
      },
      Ready => {
        debug!("Stream for {:?} can be read.", token);
        FrameEngine::read(event_loop, codec, frame_handler, token, efs, buffer_pool, outbox_pool);
      },
      Done => {
        debug!("'Readable' event on 'Done' stream. {:?}", token);
      }
    }
  }

  fn read(event_loop: &mut EventLoop<FrameEngine<E, F, C, H>>,
          codec: &mut C,
          frame_handler: &mut H,
          token: Token,
          efs: &mut EventedFrameStream<E,F>,
          buffer_pool: &mut Pool<Buffer>,
          outbox_pool: &mut Pool<Outbox<F>>) -> Result<(), Error> {
    let mut frames = Vec::new(); // TODO: Store one in FrameEngine and re-use it
    // Read bytes into a buffer
    match Self::read_bytes(efs, token, buffer_pool) {
      Ok(BytesRead(0)) => {
        debug!("No bytes were available.");
        return Ok(());
      },
      Ok(BytesRead(bytes_read)) => {
        debug!("Read {} bytes", bytes_read);
      },
      Err(io_error) => {
        error!("An I/O Error occurred while reading: {:?}", io_error);
        return Err(Io(io_error));
      }
    };

    // There are no bytes to decode
    if efs.read_buffer.is_none() || efs.read_buffer.as_mut().unwrap().len() == 0 {
      debug!("There are no bytes to decode.");
      return Ok(());
    }
    // If the read was successful, try to decode frames from the bytes
    // read_bytes 
    match Self::decode_frames(
        token,
        codec, 
        &mut efs.read_buffer.as_mut().unwrap(),
        &mut frames) {
      Ok(_) => {
        debug!("Decoding complete. {} frames decoded.", frames.len());
      },
      Err(error) => {
        error!("Encountered a protocol error: {:?}", error);
        return Err(Decoding(error));
      }
    };

    // Invoke the user's callback for each frame that was decoded
    let frame_stream = &mut FrameStream::new(efs, outbox_pool, token);
    for frame in frames { // Process each and destroy the vector
      debug!("Invoking on_frame for {:?}", token);
      frame_handler.on_frame(frame_stream, frame);
    }
    Ok(())
  }

  fn read_bytes( efs: &mut EventedFrameStream<E,F>,
                 token: Token,
                 buffer_pool: &mut Pool<Buffer>) -> io::Result<BytesRead> {
    debug!("Reading bytes for {:?}", token);
    let (stream, mut read_buffer, state) = efs.reading_toolset(buffer_pool);
    match stream.read_into_buffer(&mut read_buffer) {
      Ok(bytes_read) => Ok(BytesRead(bytes_read)),
      Err(io_error) => Err(io_error)
    }
  }

  fn decode_frames(
      token: Token,
      codec: &mut C, 
      buffer: &mut Buffer, 
      frames: &mut Vec<F>) -> Result<(), DecodingError> {
    debug!("Decoding frames for {:?}", token);
    let mut start_of_remaining: usize = 0;
    loop {
      let number_of_bytes = buffer.len();
      let filled = &buffer.bytes()[start_of_remaining..number_of_bytes];
      if start_of_remaining == number_of_bytes {
        debug!("All available bytes have been read.");
        return Ok(());
      }
      debug!("Attempting to decode range [{}..{}] of buffer", start_of_remaining, number_of_bytes);
      match codec.decode(filled) {
        Ok(decoded_frame) => {
          let BytesRead(size_in_bytes) = decoded_frame.bytes_read;
          debug!("Got a decoded frame {} bytes long", size_in_bytes);
          start_of_remaining += size_in_bytes;
          frames.push(decoded_frame.frame);
        },
        Err(DecodingError::IncompleteFrame) => {
          debug!("Found an incomplete frame.");
          return Ok(()); 
        },
        Err(error) => {
          error!("{:?} Error while decoding frame: {:?}", token, error);
          return Err(error);
        },
      };
    }
  }

  fn on_writable(event_loop: &mut EventLoop<FrameEngine<E, F, C, H>>,
           codec: &mut C,
           frame_handler: &mut H,
           token: Token,
           efs: &mut EventedFrameStream<E,F>,
           buffer_pool: &mut Pool<Buffer>,
           outbox_pool: &mut Pool<Outbox<F>>) {
    let state = efs.state;
    match state {
      NotReady => { // The 'writable' event signals a fully connected socket
        debug!("Stream for {:?} is Ready.", token);
        efs.state = Ready;
        frame_handler.on_ready(&mut FrameStream::new(efs, outbox_pool, token));
        FrameEngine::write(event_loop, codec, frame_handler, token, efs, buffer_pool, outbox_pool);
      },
      Ready => {
        trace!("Stream for {:?} can be written to.", token);
        FrameEngine::write(event_loop, codec, frame_handler, token, efs, buffer_pool, outbox_pool);
      },
      Done => {
        debug!("'Writable' event on 'Done' stream. {:?}", token); 
      }
    }
  }

  fn write(event_loop: &mut EventLoop<FrameEngine<E, F, C, H>>,
           codec: &mut C,
           frame_handler: &mut H,
           token: Token,
           efs: &mut EventedFrameStream<E,F>,
           buffer_pool: &mut Pool<Buffer>,
           outbox_pool: &mut Pool<Outbox<F>>) -> Result<(), Error> {
    match Self::encode_frames(codec, token, efs, buffer_pool, outbox_pool) {
      Ok(_) => {
        debug!("Encoding complete for {:?}", token);
      },
      Err(error) => {
        error!("Encountered an encoding error for {:?}", token);
        return Err(Encoding(error));
      } 
    };
    match Self::write_bytes(token, efs, buffer_pool, outbox_pool) {
      Ok(BytesWritten(_)) => {
        debug!("Writing complete for {:?}", token);
      },
      Err(error) => {
        error!("Encountered an I/O error for {:?}: {:?}", token, error);
        return Err(Io(error));
      }
    };
    Ok(())
  }

  // Acquire a buffer, serialize as many frames as will fit into it,
  // and then write as much of that buffer into the stream as possible without
  // blocking.
  fn encode_frames(
                  codec: &mut C,
                  token: Token,
                  efs: &mut EventedFrameStream<E,F>,
                  buffer_pool: &mut Pool<Buffer>,
                  outbox_pool: &mut Pool<Outbox<F>>) -> Result<(), EncodingError> {
   
    // References to the efs components we'll need for serializing
    let (stream, mut write_buffer, outbox, state) = efs.writing_toolset(buffer_pool, outbox_pool);

    // Serialize frames into the buffer until it's full
    // or we've run out of frames
    let mut frame_count: usize = 0;
    let mut total_bytes_written: usize = 0;
    while outbox.len() > 0 {
      let frame: F = outbox.pop_front().expect("Outbox has len>0 but no messages.");
      let buffer_to_fill = write_buffer.remaining();
      match codec.encode(&frame, buffer_to_fill) {
        Ok(BytesWritten(bytes_written)) => {
          debug!("Serialized frame as {} bytes", bytes_written);
          frame_count += 1;
          total_bytes_written += bytes_written;
        },
        Err(EncodingError::InsufficientBuffer) => {
          //TODO: Check whether the buffer is empty.
          // If it is, upgrade the buffer to a one-off
          debug!("Could not serialize frame: not enough buffer space.");
          // We need to try serializing again later
          outbox.push_front(frame);
          break;
        },
        Err(error) => {
          error!("{:?} Error while encoding frame: {:?}", token, error);
          return Err(error);
        }
      };
    }
    debug!("Serialized {} frames into the buffer.", frame_count);
    let buffer_starting_size: usize = write_buffer.len();
    let buffer_current_size: usize = buffer_starting_size + total_bytes_written;
    debug!("Write buffer grew from {} bytes to {} bytes.", buffer_starting_size, buffer_current_size);
    write_buffer.set_size(buffer_current_size);
    Ok(())
  }

  fn write_bytes(token: Token,
                 efs: &mut EventedFrameStream<E,F>,
                 buffer_pool: &mut Pool<Buffer>,
                 outbox_pool: &mut Pool<Outbox<F>>) -> io::Result<BytesWritten> {
    // TODO: This may not be necessary; values are guaranteed to be in place?
    let (stream, mut write_buffer, _outbox, _state) = efs.writing_toolset(buffer_pool, outbox_pool);

    match stream.write_from_buffer(&mut write_buffer) {
      Ok(bytes_written) => {
        debug!("{} bytes written to the bytestream.", bytes_written);
        Ok(BytesWritten(bytes_written))
      },
      Err(error) => {
        error!("{:?} Error while writing to the bytestream: {:?}", token, error);
        Err(error)
      }
    }
  }

  fn hup(event_loop: &mut EventLoop<Self>, 
           token: Token, 
           efs: &mut EventedFrameStream<E,F>
         ) {
      debug!("'Hup' event on '{:?}' stream. {:?}", efs.state, token); 
      efs.state = Done;
  }

  fn error(event_loop: &mut EventLoop<Self>, 
           token: Token, 
           efs: &mut EventedFrameStream<E,F>
          ) {
      debug!("'Error' event on '{:?}' stream. {:?}", efs.state, token); 
      efs.state = Done;
  }
}
