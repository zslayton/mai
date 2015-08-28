#![allow(dead_code)]

extern crate mio;
extern crate slab;
extern crate lifeguard;

pub mod codec;
pub mod buffer;
pub mod streams;

pub use codec::Codec;
pub use buffer::Buffer;
pub use streams::FrameEngine;
pub use streams::FrameEngineBuilder;
pub use streams::EventedByteStream;

use mio::{Evented, EventLoop};
use std::collections::BTreeMap;
use lifeguard::Pool;

const BUFFER_POOL_SIZE: usize = 16;

pub fn frame_engine<E>(max_streams: usize) -> FrameEngineBuilder<E> where E: EventedByteStream {
  FrameEngineBuilder {
    frame_engine: FrameEngine {
      streams: BTreeMap::new(),
      buffer_pool: Pool::with_size_and_max(BUFFER_POOL_SIZE, BUFFER_POOL_SIZE),
      next_token: 0
    },
    event_loop: EventLoop::new().ok().expect("EventLoop creation failed."),
  }
}

