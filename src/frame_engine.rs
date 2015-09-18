use mio::{Evented, EventSet, EventLoop as MioEventLoop, Token};
use mio::Handler as MioHandler;
use lifeguard::{Pool};

use std::thread;
use std::result::Result;
use std::io::{self, Read, Write};
use std::sync::mpsc::{channel, Sender};

use stream_manager::StreamManager;
use error::Error::{self, Io, Encoding, Decoding};

use Protocol;
use FrameEngineRemote;
use Context;
use Handler;
use EventedFrameStream;
use EventedByteStream;
use StreamState::{NotReady, Ready, Done};
use Outbox;
use Buffer;
use codec::*;
use settings::*;

pub struct FrameEngineBuilder<P: ?Sized> where P: Protocol {
  pub codec: P::Codec,
  pub handler: P::Handler,
  pub event_loop: MioEventLoop<FrameEngine<P>>,
  pub starting_buffer_size: Bytes,
  pub buffer_pool_size: usize,
  pub max_buffer_pool_size: usize,
}

pub enum Command<P: ?Sized> where P: Protocol {
  Shutdown,
  Manage(P::ByteStream),
  Send(Token, P::Frame),
//  SendFrameToList(Vec<Token>, F),
//  BroadcastFrame(F)
}

pub struct FrameEngine<P: ?Sized> where P: Protocol {
  // TODO: Add server
  pub streams: StreamManager<P>,
  pub buffer_pool: Pool<Buffer>,
  pub outbox_pool: Pool<Outbox<P::Frame>>,
  pub codec: P::Codec,
  pub handler: P::Handler,
  pub sender: Sender<()>,
}

unsafe impl <P: ?Sized> Sync for FrameEngine<P> where P : Protocol {}
unsafe impl <P: ?Sized> Send for FrameEngine<P> where P : Protocol {}

impl <P: ?Sized> FrameEngineBuilder<P> where P: Protocol + 'static {

  pub fn with<T>(self, option_setter: T) -> FrameEngineBuilder<P> where T: OptionSetter<FrameEngineBuilder<P>> {
    option_setter.set_option(self)
  }
    
  pub fn run(self) -> FrameEngineRemote<P> {
    let mut event_loop: MioEventLoop<FrameEngine<P>> = self.event_loop;
    let codec = self.codec;
    let handler = self.handler;
    let buffer_pool_size = self.buffer_pool_size;
    let max_buffer_pool_size = self.max_buffer_pool_size;
    let starting_buffer_size = self.starting_buffer_size;
    let command_sender = event_loop.channel();
    let (sender, receiver) = channel();
    let frame_engine_remote = FrameEngineRemote::new(command_sender, receiver);
    let _ = thread::spawn(move || {
      let mut frame_engine : FrameEngine<P> = FrameEngine::new(
          codec, 
          handler,
          sender,
          buffer_pool_size,
          max_buffer_pool_size,
          starting_buffer_size);
      let _ = event_loop.run(&mut frame_engine);
    });
    frame_engine_remote
  }
}

impl <P: ?Sized> MioHandler for FrameEngine<P> where P: Protocol {
  type Timeout = P::Timeout;
  type Message = Command<P>;

  fn notify(&mut self, event_loop: &mut MioEventLoop<Self>, command: Command<P>) {
    use self::Command::*;
    match command {
      Shutdown => {
        debug!("Received Shutdown command. Shutting down event loop.");
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

  fn ready(&mut self, event_loop: &mut MioEventLoop<Self>, token: Token, event_set: EventSet) {
    trace!("{:?} ready, EventSet={:?}", token, event_set);

    // Break `self` into mutable references to its components
    let FrameEngine {
      ref mut streams,
      ref mut buffer_pool,
      ref mut outbox_pool,
      ref mut codec,
      ref mut handler,
      ..
    } = *self;

    let mut stream_is_done = false;
    {
        // Get a reference to the EventedFrameStream we'll be modifying
       let mut efs: &mut EventedFrameStream<P> = streams.get_mut(token).expect("Missing token!");

        if event_set.is_writable() {
          if let Err(error) = FrameEngine::on_writable(event_loop,
                                                       codec,
                                                       handler,
                                                       token,
                                                       efs,
                                                       buffer_pool,
                                                       outbox_pool) {
            efs.on_error(event_loop, token, outbox_pool, handler, &error);
          }
          if efs.has_bytes_to_write() {
            debug!("{:?} is still waiting to write.", token);
          } else {
            debug!("No more to write for {:?}, deregistering interest in writes", token);
            if let Err(error) = efs.deregister_interest_in_writing(event_loop, token) {
              efs.on_error(event_loop, token, outbox_pool, handler, &Io(error));
            }
          }
        }

        if event_set.is_readable() {
          if let Err(error) = FrameEngine::on_readable(event_loop, 
                                                      codec, 
                                                      handler, 
                                                      token, 
                                                      efs, 
                                                      buffer_pool, 
                                                      outbox_pool) {
            efs.on_error(event_loop, token, outbox_pool, handler, &error);
          }
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
          if let Err(error) = event_loop.deregister(&efs.stream) {
            efs.on_error(event_loop, token, outbox_pool, handler, &Io(error));
          }
          handler.on_closed(&mut Context::new(event_loop, efs, outbox_pool, token));
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

  fn timeout(&mut self, _event_loop: &mut MioEventLoop<Self>, timeout: Self::Timeout) {
    debug!("Timeout fired, Timeout={:?}", timeout);
    self.handler.on_timeout(timeout);
  }

  fn interrupted(&mut self, _event_loop: &mut MioEventLoop<Self>) {
    debug!("Interrupted");
  }
}

impl <P: ?Sized> FrameEngine<P> where P: Protocol {
  pub fn new(codec: P::Codec, handler: P::Handler, sender: Sender<()>, buffer_pool_size: usize, max_buffer_pool_size: usize, _starting_buffer_size: Bytes) -> FrameEngine<P> {
    //TODO: Use the starting_buffer_size setting. See issue #5.
    FrameEngine {
      streams: StreamManager::new(),
      buffer_pool: Pool::with_size_and_max(buffer_pool_size, max_buffer_pool_size),
      outbox_pool: Pool::with_size_and_max(buffer_pool_size, max_buffer_pool_size),
      codec: codec,
      handler: handler,
      sender: sender
    }
  }

  pub fn manage(&mut self, event_loop: &mut MioEventLoop<FrameEngine<P>>, evented_byte_stream: P::ByteStream) -> Token {
    let evented_frame_stream = EventedFrameStream::new(evented_byte_stream);
    let token = self.streams.insert(evented_frame_stream);
    let efs = self.streams.get_mut(token).expect("Missing just-inserted EventedFrameStream");
    //TODO: Handle error?
    let _ = event_loop.register(&efs.stream, token).unwrap();
    token
  }

  pub fn send(&mut self, event_loop: &mut MioEventLoop<FrameEngine<P>>, token: Token, frame: P::Frame) -> io::Result<()> { //TODO: Formal error type
    let FrameEngine {
      ref mut streams,
      ref mut outbox_pool,
      ..
    } = *self;
    // Get the appropriate efs
    // TODO: Break this into its own helper function
    let efs: &mut EventedFrameStream<P> = match streams.get_mut(token) {
      Some(efs) => efs,
      None => return Err(io::Error::new(io::ErrorKind::NotFound, "No such token."))
    };
   
    match efs.send(event_loop, token, outbox_pool, frame) {
      Ok(_) => Ok(()),
      Err(error) => Err(error)
    }
  }

  fn on_readable(event_loop: &mut MioEventLoop<FrameEngine<P>>,
          codec: &mut P::Codec,
          handler: &mut P::Handler,
          token: Token,
          efs: &mut EventedFrameStream<P>, 
          buffer_pool: &mut Pool<Buffer>,
          outbox_pool: &mut Pool<Outbox<P::Frame>>) -> Result<(), Error> {
    match efs.state {
      NotReady => { // The 'readable' event signals a fully connected socket
        debug!("Stream for {:?} is now Ready.", token);
        efs.state = Ready;
        handler.on_ready(&mut Context::new(event_loop, efs, outbox_pool, token));
        FrameEngine::read(event_loop, codec, handler, token, efs, buffer_pool, outbox_pool)
      },
      Ready => {
        debug!("Stream for {:?} can be read.", token);
        FrameEngine::read(event_loop, codec, handler, token, efs, buffer_pool, outbox_pool)
      },
      Done => {
        debug!("'Readable' event on 'Done' stream. {:?}", token);
        Ok(())
      }
    }
  }

  fn read(event_loop: &mut MioEventLoop<FrameEngine<P>>,
          codec: &mut P::Codec,
          handler: &mut P::Handler,
          token: Token,
          efs: &mut EventedFrameStream<P>, 
          buffer_pool: &mut Pool<Buffer>,
          outbox_pool: &mut Pool<Outbox<P::Frame>>) -> Result<(), Error> {
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
      Err(decoding_error) => {
        error!("Encountered a decoding error: {:?}", decoding_error);
        return Err(Decoding(decoding_error));
      }
    };

    // Invoke the user's callback for each frame that was decoded
    let context = &mut Context::new(event_loop, efs, outbox_pool, token);
    for frame in frames { // Process each and destroy the vector
      debug!("Invoking on_frame for {:?}", token);
      handler.on_frame(context, frame);
    }
    Ok(())
  }

  fn read_bytes( efs: &mut EventedFrameStream<P>, 
                 token: Token,
                 buffer_pool: &mut Pool<Buffer>) -> io::Result<BytesRead> {
    debug!("Reading bytes for {:?}", token);
    let (stream, mut read_buffer) = efs.reading_toolset(buffer_pool);
    match stream.read_into_buffer(&mut read_buffer) {
      Ok(bytes_read) => Ok(BytesRead(bytes_read)),
      Err(io_error) => Err(io_error)
    }
  }

  fn decode_frames(
      token: Token,
      codec: &mut P::Codec, 
      buffer: &mut Buffer, 
      frames: &mut Vec<P::Frame>) -> Result<(), DecodingError> {
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

  fn on_writable(event_loop: &mut MioEventLoop<FrameEngine<P>>,
           codec: &mut P::Codec,
           handler: &mut P::Handler,
           token: Token,
           efs: &mut EventedFrameStream<P>, 
           buffer_pool: &mut Pool<Buffer>,
           outbox_pool: &mut Pool<Outbox<P::Frame>>) -> Result<(), Error> {
    let state = efs.state;
    match state {
      NotReady => { // The 'writable' event signals a fully connected socket
        debug!("Stream for {:?} is Ready.", token);
        efs.state = Ready;
        handler.on_ready(&mut Context::new(event_loop, efs, outbox_pool, token));
        FrameEngine::write(codec, token, efs, buffer_pool, outbox_pool)
      },
      Ready => {
        trace!("Stream for {:?} can be written to.", token);
        FrameEngine::write(codec, token, efs, buffer_pool, outbox_pool)
      },
      Done => {
        debug!("'Writable' event on 'Done' stream. {:?}", token); 
        Ok(())
      }
    }
  }

  fn write(codec: &mut P::Codec,
           token: Token,
           efs: &mut EventedFrameStream<P>, 
           buffer_pool: &mut Pool<Buffer>,
           outbox_pool: &mut Pool<Outbox<P::Frame>>) -> Result<(), Error> {
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
      Err(io_error) => {
        error!("Encountered an I/O error for {:?}: {:?}", token, io_error);
        return Err(Io(io_error));
      }
    };
    Ok(())
  }

  // Acquire a buffer, serialize as many frames as will fit into it,
  // and then write as much of that buffer into the stream as possible without
  // blocking.
  fn encode_frames(
                  codec: &mut P::Codec,
                  token: Token,
                  efs: &mut EventedFrameStream<P>,
                  buffer_pool: &mut Pool<Buffer>,
                  outbox_pool: &mut Pool<Outbox<P::Frame>>) -> Result<(), EncodingError> {
   
    // References to the efs components we'll need for serializing
    let (_stream, mut write_buffer, outbox) = efs.writing_toolset(buffer_pool, outbox_pool);

    // Serialize frames into the buffer until it's full
    // or we've run out of frames
    let mut frame_count: usize = 0;
    let mut total_bytes_written: usize = 0;
    while outbox.len() > 0 {
      let frame: P::Frame = outbox.pop_front().expect("Outbox has len>0 but no messages.");
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
                 efs: &mut EventedFrameStream<P>, 
                 buffer_pool: &mut Pool<Buffer>,
                 outbox_pool: &mut Pool<Outbox<P::Frame>>) -> io::Result<BytesWritten> {
    // TODO: This may not be necessary; values are guaranteed to be in place?
    let (stream, mut write_buffer, _outbox) = efs.writing_toolset(buffer_pool, outbox_pool);

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

  fn hup(_event_loop: &mut MioEventLoop<Self>, 
           token: Token, 
           efs: &mut EventedFrameStream<P>
         ) {
      debug!("'Hup' event on '{:?}' stream. {:?}", efs.state, token); 
      efs.state = Done;
  }

  fn error(_event_loop: &mut MioEventLoop<Self>, 
           token: Token, 
           efs: &mut EventedFrameStream<P>
          ) {
      debug!("'Error' event on '{:?}' stream. {:?}", efs.state, token); 
      efs.state = Done;
  }
}
