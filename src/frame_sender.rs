use lifeguard::{RcRecycled};

use std::collections::VecDeque;

use EventedByteStream;
use Buffer;

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
    // Has bytes in the outbount buffer waiting to be written...
    (!self.write_buffer.is_none() && self.write_buffer.get().len() > 0) 
    // or frames waiting to be serialized and written
        || (!self.outbox.is_none() && self.outbox.get().len() > 0)
  }
}

