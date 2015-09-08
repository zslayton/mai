use mio::{Evented, EventSet, PollOpt, EventLoop, Handler, Token};
use lifeguard::{Pool, RcRecycled};

use std::marker::PhantomData;
use std::io::{self, Read, Write, ErrorKind};
use std::borrow::BorrowMut;
use std::collections::VecDeque;

use slab::Index;

use stream_manager::StreamManager;
use error::Error::{self, Io, Encoding, Decoding};
//use error::Error::*;

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
  H: FrameHandler<F>,
  F: Send
{
  pub frame_engine: FrameEngine<E, F, C, H>, // TODO: make private?
  pub event_loop: EventLoop<FrameEngine<E, F, C, H>>,
  pub sender: EventLoop<FrameEngine<E, F, C, H>>
  // FINISH ADDING SENDER/RECEIVER TO FEB,
  // migrate them in FE::run(), test it out
  // add timers!
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
  H: FrameHandler<F>
{
  // TODO: Add server
  pub streams: StreamManager<E, F>, //TODO: Make this a BTreeMap?
  pub buffer_pool: Pool<Buffer>,
  pub outbox_pool: Pool<Outbox<F>>,
  pub codec: C,
  pub frame_handler: H,
  pub sender: Sender<()>,
  frame_type: PhantomData<F> //TODO: make a FrameEngine trait so this can be an assoc type?
}

impl <E, F, C, H> FrameEngineBuilder<E, F, C, H> where
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<F>,
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

  pub fn run(self) -> Sender<Command<E,F>>{
    let mut event_loop: EventLoop<FrameEngine<E, F, C, H>> = self.event_loop;
    let mut frame_engine : FrameEngine<E, F, C, H> = self.frame_engine;
    let (tx, rx): (Sender<()>, Receiver<()>) = channel();
    let command_sender = event_loop.channel();
    let _ = event_loop.run(&mut frame_engine); 
    (command_sender, rx); // TODO: Clarify names
  }
}

impl <E, F, C, H> Handler for FrameEngine<E, F, C, H> where
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<F>,
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
      ref mut sender,
      ref frame_type
    } = *self;

    let mut stream_is_done = false;
    {
        // Get a reference to the EventedFrameStream we'll be modifying
        let mut efs: &mut EventedFrameStream<E,F> = streams.get_mut(token).expect("Missing token!");

        if event_set.is_writable() {
          FrameEngine::write(codec, frame_handler, token, efs, buffer_pool, outbox_pool);
          if efs.has_bytes_to_write() {
            debug!("{:?} is still waiting to write.", token);
          } else {
            debug!("No more to write for {:?}, deregistering interest in writes", token);
            let mut interests = EventSet::all();
            interests.remove(EventSet::writable());
            let poll_opt = PollOpt::level();
            event_loop.reregister(&efs.stream, token, interests, poll_opt);
          }
        }

        if event_set.is_readable() {
          FrameEngine::read(codec, frame_handler, token, efs, buffer_pool);
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
          frame_handler.on_closed(token);
          stream_is_done = true;
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
  H: FrameHandler<F>,
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
      ref mut sender,
      ref frame_type
    } = *self;
    // Get the appropriate efs
    // TODO: Break this into its own helper function
    let efs: &mut EventedFrameStream<E,F> = match streams.get_mut(token) {
      Some(efs) => efs,
      None => return Err(FrameEngineError::NoSuchToken)
    };
    
    let was_waiting_to_write = efs.has_bytes_to_write();

    let mut outbox = efs.outbox
        .take()
        .or_else(|| Some(outbox_pool.new_rc()))
        .expect("Couldn't get an outbox!");

    outbox.push_back(frame);
    efs.outbox = Some(outbox);

    // If we weren't waiting to write before this, register interest
    // in case we had deregistered it previously.
    if !was_waiting_to_write { 
      debug!("Registering interest in writable event for {:?}", token);
      event_loop.reregister(&efs.stream, token, EventSet::all(), PollOpt::level());
    }
    debug!("New message in outbox for {:?}", token);

    Ok(())
  }

  fn read( codec: &mut C, 
           frame_handler: &mut H,
           token: Token, 
           efs: &mut EventedFrameStream<E,F>,
           buffer_pool: &mut Pool<Buffer>) {
    match efs.state {
      NotReady => { // The 'readable' event signals a fully connected socket
        debug!("Stream for {:?} is now Ready.", token);
        efs.state = Ready;
        frame_handler.on_ready(token);
        FrameEngine::read_frames(codec, frame_handler, token, efs, buffer_pool); 
      },
      Ready => {
        debug!("Stream for {:?} can be read.", token);
        FrameEngine::read_frames(codec, frame_handler, token, efs, buffer_pool); 
      },
      Done => {
        debug!("'Readable' event on 'Done' stream. {:?}", token); 
      }
    }
  }

  // Acquire a buffer, read as many bytes as possible into it without blocking,
  // and then parse as many frames as possible from those bytes, invoking the
  // necessary callback each time.
  fn read_frames(codec: &mut C,
                 frame_handler: &mut H,
                 token: Token,
                 efs: &mut EventedFrameStream<E,F>,
                 buffer_pool: &mut Pool<Buffer>) {
    let mut read_buffer = efs.read_buffer
      .take()
      .or_else(|| Some(buffer_pool.new_rc()))
      .expect("Couldn't get a read buffer!");
    let mut frame_count : usize = 0;
    match efs.stream.read_into_buffer(&mut read_buffer) {
      Ok(bytes_read) => {
        debug!("{} bytes read.", bytes_read);
        let mut start_of_remaining: usize = 0;
        loop {
          let filled = &read_buffer.bytes()[start_of_remaining..bytes_read];
          if start_of_remaining == bytes_read {
            debug!("Read all available bytes.");
            break;
          }
          debug!("Attempting to decode range [{}..{}] of buffer", start_of_remaining, bytes_read);
          let frame = match codec.decode(filled) {
            Ok(decoded_frame) => {
              let BytesRead(size_in_bytes) = decoded_frame.bytes_read;
              debug!("Got a decoded frame {} bytes long", size_in_bytes);
              start_of_remaining += size_in_bytes;
              decoded_frame.frame
            },
            Err(DecodingError::IncompleteFrame) => break,
            Err(error) => {
              error!("{:?} Error while decoding frame: {:?}", token, error);
              frame_handler.on_error(token, Decoding(error));
              efs.state = Done;
              return;
            },
          };
          frame_count += 1;
          debug!("Calling frame handler");
          frame_handler.on_frame_received(token, frame);
        }
      },
      Err(error) => {
        error!("{:?} Error while reading from bytestream: {:?}", token, error);
        frame_handler.on_error(token, Io(error));
        efs.state = Done;
        return;
      }
    };
    debug!("Read and processed {} frames.", frame_count);
  }

  fn write(codec: &mut C,
           frame_handler: &mut H,
           token: Token,
           efs: &mut EventedFrameStream<E,F>,
           buffer_pool: &mut Pool<Buffer>,
           outbox_pool: &mut Pool<Outbox<F>>) {
    match efs.state {
      NotReady => { // The 'writable' event signals a fully connected socket
        debug!("Stream for {:?} is Ready.", token);
        efs.state = Ready;
        frame_handler.on_ready(token);
        FrameEngine::write_frames(codec, frame_handler, token, efs, buffer_pool, outbox_pool);
      },
      Ready => {
        trace!("Stream for {:?} can be written to.", token);
        FrameEngine::write_frames(codec, frame_handler, token, efs, buffer_pool, outbox_pool);
      },
      Done => {
        debug!("'Writable' event on 'Done' stream. {:?}", token); 
      }
    }
  }

  // Acquire a buffer, serialize as many frames as will fit into it,
  // and then write as much of that buffer into the stream as possible without
  // blocking.
  fn write_frames(codec: &mut C,
                  frame_handler: &mut H,
                  token: Token,
                  efs: &mut EventedFrameStream<E,F>,
                  buffer_pool: &mut Pool<Buffer>,
                  outbox_pool: &mut Pool<Outbox<F>>) {
    if efs.outbox.is_none() && efs.write_buffer.is_none() {
  //debug!("No writes pending for {:?}, yielding", token);
      return;
    };

    // Either continue using this efs' write buffer
    // or retrieve a new one from the pool
    let mut write_buffer = efs.write_buffer
      .take()
      .or_else(|| Some(buffer_pool.new_rc()))
      .expect("Couldn't get a write buffer!");

    // Similarly, get a frame queue to work with
    let mut outbox = efs.outbox
        .take()
        .or_else(|| Some(outbox_pool.new_rc()))
        .expect("Couldn't get an outbox!");

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
          frame_handler.on_error(token, Encoding(error));
          efs.state = Done;
          return;
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

    match efs.stream.write_from_buffer(&mut write_buffer) {
      Ok(bytes_written) => {
        debug!("{} bytes written to the bytestream.", bytes_written);
      },
      Err(error) => {
        error!("{:?} Error while writing to the bytestream: {:?}", token, error);
        frame_handler.on_error(token, Io(error));
        efs.state = Done;
        return;
      }
    };
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
