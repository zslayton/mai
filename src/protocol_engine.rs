use mio::{Evented, EventSet, PollOpt, Token};
use mio::EventLoop as MioEventLoop;
use mio::Handler as MioHandler;
use mio::Sender as MioSender;
use lifeguard::{self, Recycleable, Pool, StartingSize, MaxSize, Supplier};

use std::result::Result;
use std::io::{self, Read, Write};

use stream_manager::StreamManager;
use stream_session::StreamSession;
use error::Error::{self, Io, Encoding, Decoding};

use Protocol;
use Context;
use context::EngineHandle;
use Handler;
use EventedFrameStream;
use EventedByteStream;
use StreamState::{NotReady, Ready, Done};
use Outbox;
use Buffer;
use timeout::Timeout;
use codec::*;
use settings::*;

pub struct ProtocolEngineBuilder<P: ?Sized> where P: Protocol {
  pub handler: P::Handler,
  pub event_loop: MioEventLoop<ProtocolEngine<P>>,
  pub starting_buffer_size: Bytes,
  pub buffer_pool_size: usize,
  pub max_buffer_pool_size: usize,
}

pub enum Command<P: ?Sized> where P: Protocol {
  Shutdown,
  Manage(P::ByteStream, P::Session),
  Send(Token, P::Frame),
//  SendFrameToList(Vec<Token>, F),
  Broadcast(P::Frame)
}

pub struct ProtocolEngine<P: ?Sized> where P: Protocol {
  // TODO: Add server
  pub event_loop: Option<MioEventLoop<ProtocolEngine<P>>>,
  pub streams: StreamManager<P>,
  pub buffer_pool: Pool<Buffer>,
  pub outbox_pool: Pool<Outbox<P::Frame>>,
  pub handler: P::Handler,
  pub command_sender: MioSender<Command<P>>,
}

// unsafe impl <P: ?Sized> Sync for ProtocolEngine<P> where P : Protocol {}
// unsafe impl <P: ?Sized> Send for ProtocolEngine<P> where P : Protocol {}

impl <P: ?Sized> ProtocolEngineBuilder<P> where P: Protocol + 'static {

  pub fn with<T>(self, option_setter: T) -> ProtocolEngineBuilder<P> where T: OptionSetter<ProtocolEngineBuilder<P>> {
    option_setter.set_option(self)
  }

  pub fn build(self) -> ProtocolEngine<P> {
    let command_sender = self.event_loop.channel();
    let protocol_engine : ProtocolEngine<P> = ProtocolEngine::new(
        self.event_loop,
        self.handler,
        command_sender,
        self.buffer_pool_size,
        self.max_buffer_pool_size,
        self.starting_buffer_size
    );
    protocol_engine
  }
}

impl <P: ?Sized> MioHandler for ProtocolEngine<P> where P: Protocol {
  type Timeout = Timeout<P::Timeout>;
  type Message = Command<P>;

  fn notify(&mut self, event_loop: &mut MioEventLoop<Self>, command: Command<P>) {
    use self::Command::*;
    match command {
      Shutdown => {
        debug!("Received Shutdown command. Shutting down event loop.");
        event_loop.shutdown();
      },
      Manage(evented_byte_stream, session) => {
        debug!("Received Manage command. Registering new EventedByteStream.");
        let streams = &mut self.streams;
        Self::manage_stream(streams, event_loop, evented_byte_stream, session);
      },
      Send(token, frame) => {
        debug!("Received Send command. Sending frame to {:?}", token);
        let (streams, outbox_pool) = (&mut self.streams, &mut self.outbox_pool);
        match Self::send_frame(streams, outbox_pool, event_loop, token, frame) {
          Err(error) => error!("Failed to send to {:?}: {:?}", token, error),
          _ => {}
        }
      },
      Broadcast(frame) => {
        debug!("Received Broadcast command. Sending frame to all streams.");
        match self.broadcast(event_loop, frame) {
          Err(error) => error!("Failed to broadcast message: {:?}", error),
          _ => {}
        }
      }
    }
  }

  fn ready(&mut self, event_loop: &mut MioEventLoop<Self>, token: Token, event_set: EventSet) {
    trace!("{:?} ready, EventSet={:?}", token, event_set);

    // Break `self` into mutable references to its components
    let ProtocolEngine {
      ref mut streams,
      ref mut buffer_pool,
      ref mut outbox_pool,
      ref mut handler,
      ref mut command_sender,
      ..
    } = *self;

    let mut stream_is_done = false;
    {
        // Get a reference to the EventedFrameStream we'll be modifying
       let stream_session: Option<&mut StreamSession<P>> = streams.get_mut(token);
       if stream_session.is_none() {
           warn!("Received a ready event for an invalid token: {:?}", token);
           return;
       }
       let mut stream_session: &mut StreamSession<P> = stream_session.unwrap();

       let (mut efs, mut session) = stream_session.components();

        if event_set.is_writable() {
          if let Err(error) = ProtocolEngine::on_writable(event_loop,
                                                       handler,
                                                       token,
                                                       efs,
                                                       session,
                                                       buffer_pool,
                                                       outbox_pool,
                                                       command_sender) {
            efs.on_error(event_loop, token, session, outbox_pool, command_sender, handler, &error);
          }
          if efs.has_bytes_to_write() {
            debug!("{:?} is still waiting to write.", token);
          } else {
            debug!("No more to write for {:?}, deregistering interest in writes", token);
            if let Err(error) = efs.deregister_interest_in_writing(event_loop, token) {
              efs.on_error(event_loop, token, session, outbox_pool, command_sender, handler, &Io(error));
            }
          }
        }

        if event_set.is_readable() {
          if let Err(error) = ProtocolEngine::on_readable(event_loop,
                                                      handler,
                                                      token,
                                                      efs,
                                                      session,
                                                      buffer_pool,
                                                      outbox_pool,
                                                      command_sender) {
            efs.on_error(event_loop, token, session, outbox_pool, command_sender, handler, &error);
          }
        }

        if event_set.is_hup() {
          ProtocolEngine::hup(event_loop, token, efs);
        }

        if event_set.is_error() {
          ProtocolEngine::error(event_loop, token, efs);
        }

        // Because `efs` borrows `streams`, we need to process the end
        // of this stream's life in two phases: 1) unregister `efs` from
        // the event_loop and allow it to drop, un-borrowing `streams`
        // and 2) remove the stream itself from the protocol_engine
        if let Done = efs.state {
          debug!("Shutting down stream. {:?}", token);
          if let Err(error) = event_loop.deregister(&efs.stream) {
            efs.on_error(event_loop, token, session, outbox_pool, command_sender, handler, &Io(error));
          }
          handler.on_closed(&mut Context::new(event_loop, efs, session, outbox_pool, command_sender, token));
          stream_is_done = true;
        } else {
          efs.release_empty_buffers();
        }
    } // `streams` is no longer borrowed

    if stream_is_done {
      debug!("Removing stream from ProtocolEngine");
      let _ = streams.remove(token); // Drop the stream, freeing up pooled resources
    }
  }

  fn timeout(&mut self, event_loop: &mut MioEventLoop<Self>, timeout: Timeout<P::Timeout>) {
    debug!("Timeout fired, Timeout={:?}", timeout);
    match timeout.token {
        Some(token) => {
            let ProtocolEngine {
              ref mut streams,
              ref mut outbox_pool,
              ref mut handler,
              ref mut command_sender,
              ..
            } = *self;
            let stream_session: Option<&mut StreamSession<P>> = streams.get_mut(token);
            if stream_session.is_none() {
                warn!("Received a timeout for an invalid token: {:?}", token);
                return;
            }
            let mut stream_session: &mut StreamSession<P> = stream_session.unwrap();
            let (mut efs, mut session) = stream_session.components();
            handler.on_timeout(
                &mut Context::new(event_loop, efs, session, outbox_pool, command_sender, token),
                timeout.data
            );
        },
        None => {
            let (ref mut event_loop, ref mut command_sender) = (event_loop, &mut self.command_sender);
            let engine_handle = EngineHandle::new(event_loop, command_sender);
            self.handler.on_global_timeout(engine_handle, timeout.data);
        }
    }

  }

  fn interrupted(&mut self, _event_loop: &mut MioEventLoop<Self>) {
    debug!("Interrupted");
    //TODO: Expose/Propagate?
  }
}

impl <P: ?Sized> ProtocolEngine<P> where P: Protocol {
  pub fn new(event_loop: MioEventLoop<Self>, handler: P::Handler, command_sender: MioSender<Command<P>>, buffer_pool_size: usize, max_buffer_pool_size: usize, starting_buffer_size: Bytes) -> ProtocolEngine<P> {
    let Bytes(starting_buffer_size) = starting_buffer_size;
    let buffer_pool = lifeguard::pool()
        .with(StartingSize(buffer_pool_size))
        .with(MaxSize(max_buffer_pool_size))
        .with(Supplier(move || Buffer::with_capacity(starting_buffer_size)))
        .build();

    let outbox_pool: Pool<Outbox<P::Frame>> = lifeguard::pool()
        .with(StartingSize(buffer_pool_size))
        .with(MaxSize(max_buffer_pool_size))
        .build();

    ProtocolEngine {
      event_loop: Some(event_loop),
      streams: StreamManager::new(),
      buffer_pool: buffer_pool,
      outbox_pool: outbox_pool,
      handler: handler,
      command_sender: command_sender
    }
  }

  pub fn channel(&mut self) -> MioSender<Command<P>> {
    self.command_sender.clone()
  }

  pub fn run(mut self) -> io::Result<()> {
    let mut event_loop = self.event_loop.take().unwrap();
    event_loop.run(&mut self)
  }

  pub fn manage(&mut self, evented_byte_stream: P::ByteStream, session: P::Session) -> Token {
    let(streams, event_loop) = (&mut self.streams, self.event_loop.as_mut().unwrap());
    Self::manage_stream(streams, event_loop, evented_byte_stream, session)
  }

  fn manage_stream(
      streams: &mut StreamManager<P>,
      event_loop: &mut MioEventLoop<ProtocolEngine<P>>,
      evented_byte_stream: P::ByteStream,
      session: P::Session
    ) -> Token {
    let evented_frame_stream = EventedFrameStream::new(evented_byte_stream);
    let stream_session = StreamSession::new(evented_frame_stream, session);
    let token = streams.insert(stream_session);
    let stream_session = streams.get_mut(token).expect("Missing just-inserted EventedFrameStream");
    //TODO: Handle error?
    let _ = event_loop.register(
            &stream_session.stream().stream,
            token,
            EventSet::all(),
            PollOpt::level()
        ).unwrap();
    token
  }

  fn broadcast(&mut self, _event_loop: &mut MioEventLoop<ProtocolEngine<P>>, _frame: P::Frame) -> io::Result<()> {
      Ok(())
//    for token in self
//    //TODO: Expose stream iteration in the stream manager, then finish this
  }

  pub fn send_frame(
      streams: &mut StreamManager<P>,
      outbox_pool: &mut Pool<Outbox<P::Frame>>,
      event_loop: &mut MioEventLoop<ProtocolEngine<P>>,
      token: Token,
      frame: P::Frame) -> io::Result<()> {
    let stream_session: &mut StreamSession<P> = match streams.get_mut(token) {
      Some(ss) => ss,
      None => return Err(io::Error::new(io::ErrorKind::NotFound, "No such token."))
    };
    match stream_session.stream().send(event_loop, token, outbox_pool, frame) {
      Ok(_) => Ok(()),
      Err(error) => Err(error)
    }
  }

  pub fn send(&mut self, token: Token, frame: P::Frame) -> io::Result<()> {
    let ProtocolEngine {
      ref mut streams,
      ref mut outbox_pool,
      ref mut event_loop,
      ..
    } = *self;
    Self::send_frame(streams, outbox_pool, event_loop.as_mut().unwrap(), token, frame)
  }

  fn on_readable(event_loop: &mut MioEventLoop<ProtocolEngine<P>>,
          handler: &mut P::Handler,
          token: Token,
          efs: &mut EventedFrameStream<P>,
          session: &mut P::Session,
          buffer_pool: &mut Pool<Buffer>,
          outbox_pool: &mut Pool<Outbox<P::Frame>>,
          command_sender: &mut MioSender<Command<P>>) -> Result<(), Error> {
    match efs.state {
      NotReady => { // The 'readable' event signals a fully connected socket
        debug!("Stream for {:?} is now Ready.", token);
        efs.state = Ready;
        handler.on_ready(&mut Context::new(event_loop, efs, session, outbox_pool, command_sender, token));
        ProtocolEngine::read(event_loop, handler, token, efs, session, buffer_pool, outbox_pool, command_sender)
      },
      Ready => {
        debug!("Stream for {:?} can be read.", token);
        ProtocolEngine::read(event_loop, handler, token, efs, session, buffer_pool, outbox_pool, command_sender)
      },
      Done => {
        debug!("'Readable' event on 'Done' stream. {:?}", token);
        Ok(())
      }
    }
  }

  fn read(event_loop: &mut MioEventLoop<ProtocolEngine<P>>,
          handler: &mut P::Handler,
          token: Token,
          efs: &mut EventedFrameStream<P>,
          session: &mut P::Session,
          buffer_pool: &mut Pool<Buffer>,
          outbox_pool: &mut Pool<Outbox<P::Frame>>,
          command_sender: &mut MioSender<Command<P>>) -> Result<(), Error> {
    let mut frames = Vec::new(); // TODO: Store one in ProtocolEngine and re-use it
    // Read bytes into a buffer
    match Self::read_bytes(efs, token, buffer_pool) {
      Ok(BytesRead(0)) => {
        debug!("No bytes were available.");
        return Ok(());
      },
      Ok(BytesRead(bytes_read)) => {
        debug!("{} bytes are available", bytes_read);
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
    { // efs borrow scope
      let (codec, read_buffer): (&mut P::Codec, &mut Buffer) = (&mut efs.codec, &mut efs.read_buffer.as_mut().unwrap());
      match Self::decode_frames(
          codec,
          token,
          read_buffer,
          &mut frames) {
        Ok(_) => {
          debug!("Decoding complete. {} frames decoded.", frames.len());
        },
        Err(decoding_error) => {
          error!("Encountered a decoding error: {:?}", decoding_error);
          return Err(Decoding(decoding_error));
        }
      };
    }

    // Invoke the user's callback for each frame that was decoded
    let context = &mut Context::new(event_loop, efs, session, outbox_pool, command_sender, token);
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
    let (_codec, stream, mut read_buffer) = efs.reading_toolset(buffer_pool);
    match stream.read_into_buffer(&mut read_buffer) {
      Ok(bytes_read) => Ok(BytesRead(bytes_read)),
      Err(io_error) => Err(io_error)
    }
  }

  fn decode_frames(
      codec: &mut P::Codec,
      token: Token,
      buffer: &mut Buffer,
      frames: &mut Vec<P::Frame>) -> Result<(), DecodingError> {
    debug!("Decoding frames for {:?}", token);
    let mut start_of_remaining: usize = 0;
    loop {
      let number_of_bytes = buffer.len();
      let decode_result;
      { // Extra scope to drop 'filled' after decoding
          if start_of_remaining == number_of_bytes {
            debug!("All available bytes have been read.");
            debug!("Setting buffer size to zero");
            buffer.set_size(0); // TODO: Refactor to get this next to other set_size() call?
            return Ok(());
          }
          let filled = &buffer.bytes()[start_of_remaining..number_of_bytes];
          debug!("Attempting to decode range [{}..{}] of buffer", start_of_remaining, number_of_bytes);
          decode_result = codec.decode(filled);
      }

      match decode_result {
        Ok(decoded_frame) => {
          let BytesRead(size_in_bytes) = decoded_frame.bytes_read;
          debug!("Got a decoded frame {} bytes long", size_in_bytes);
          start_of_remaining += size_in_bytes;
          frames.push(decoded_frame.frame);
        },
        Err(DecodingError::IncompleteFrame) => {
          let bytes_remaining = number_of_bytes - start_of_remaining;
          debug!("Found an incomplete frame. ({} bytes)", bytes_remaining);

          debug!("Restacking {} bytes in the read buffer", bytes_remaining);
          buffer.restack(start_of_remaining);

          buffer.set_size(bytes_remaining);
          return Ok(());
        },
        Err(error) => {
          error!("{:?} Error while decoding frame: {:?}", token, error);
          return Err(error);
        },
      };
    }
  }

  fn on_writable(event_loop: &mut MioEventLoop<ProtocolEngine<P>>,
           handler: &mut P::Handler,
           token: Token,
           efs: &mut EventedFrameStream<P>,
           session: &mut P::Session,
           buffer_pool: &mut Pool<Buffer>,
           outbox_pool: &mut Pool<Outbox<P::Frame>>,
           command_sender: &mut MioSender<Command<P>>) -> Result<(), Error> {
    let state = efs.state;
    match state {
      NotReady => { // The 'writable' event signals a fully connected socket
        debug!("Stream for {:?} is Ready.", token);
        efs.state = Ready;
        handler.on_ready(&mut Context::new(event_loop, efs, session, outbox_pool, command_sender, token));
        ProtocolEngine::write(token, efs, buffer_pool, outbox_pool)
      },
      Ready => {
        trace!("Stream for {:?} can be written to.", token);
        ProtocolEngine::write(token, efs, buffer_pool, outbox_pool)
      },
      Done => {
        debug!("'Writable' event on 'Done' stream. {:?}", token);
        Ok(())
      }
    }
  }

  fn write(token: Token,
           efs: &mut EventedFrameStream<P>,
           buffer_pool: &mut Pool<Buffer>,
           outbox_pool: &mut Pool<Outbox<P::Frame>>) -> Result<(), Error> {
    match Self::encode_frames(token, efs, buffer_pool, outbox_pool) {
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
  #[allow(unused_variables)]
  fn encode_frames(
                  token: Token,
                  efs: &mut EventedFrameStream<P>,
                  buffer_pool: &mut Pool<Buffer>,
                  outbox_pool: &mut Pool<Outbox<P::Frame>>) -> Result<(), EncodingError> {

    // References to the efs components we'll need for serializing
    let (codec, _stream, mut write_buffer, outbox) = efs.writing_toolset(buffer_pool, outbox_pool);

    // Serialize frames into the buffer until it's full
    // or we've run out of frames
    let mut frame_count: usize = 0;
    let mut total_bytes_written: usize = 0;
    let buffer_starting_size: usize = write_buffer.len();
    while outbox.len() > 0 {
      let frame: P::Frame = outbox.pop_front().expect("Outbox has len>0 but no messages.");
      match codec.encode(&frame, write_buffer.remaining()) {
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

      let buffer_current_size: usize = buffer_starting_size + total_bytes_written;
      debug!("Write buffer grew to {} bytes.", buffer_current_size);
      write_buffer.set_size(buffer_current_size);
    }
    Ok(())
  }

  fn write_bytes(token: Token,
                 efs: &mut EventedFrameStream<P>,
                 buffer_pool: &mut Pool<Buffer>,
                 outbox_pool: &mut Pool<Outbox<P::Frame>>) -> io::Result<BytesWritten> {
    // TODO: This may not be necessary; values are guaranteed to be in place?
    let (_codec, stream, mut write_buffer, _outbox) = efs.writing_toolset(buffer_pool, outbox_pool);

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
