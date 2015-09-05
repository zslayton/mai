use lifeguard::{RcRecycled};

use std::collections::VecDeque;

use EventedByteStream;
use Buffer;

#[derive(Debug,Clone,Copy)]
pub enum StreamState {
  NotReady,
  Ready,
  Done
}

pub type Outbox<F> = VecDeque<F>;

#[derive(Debug)]
pub struct EventedFrameStream<E, F> where E: EventedByteStream {
  pub stream: E,
  pub state: StreamState,
  pub read_buffer: Option<RcRecycled<Buffer>>,
  pub write_buffer: Option<RcRecycled<Buffer>>,
  pub outbox: Option<RcRecycled<Outbox<F>>>
}

impl <E, F> EventedFrameStream<E, F> where E: EventedByteStream {
  pub fn new(ebs: E) -> EventedFrameStream<E, F> {
    EventedFrameStream {
      stream: ebs,
      state: StreamState::NotReady,
      read_buffer: None,
      write_buffer: None,
      outbox: None
    }
  }

  pub fn has_bytes_to_write(&self) -> bool {
    (!self.write_buffer.is_none()) || (!self.outbox.is_none())
  }
}

