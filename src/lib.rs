#![allow(dead_code)]

#[macro_use]
extern crate log;
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

use mio::{EventLoop};
use mio::util::Slab;
use lifeguard::Pool;

const BUFFER_POOL_SIZE: usize = 16;

pub fn frame_engine<E>(max_streams: usize) -> FrameEngineBuilder<E> where E: EventedByteStream {
  FrameEngineBuilder {
    frame_engine: FrameEngine {
      streams: Slab::new(max_streams),
      buffer_pool: Pool::with_size_and_max(BUFFER_POOL_SIZE, BUFFER_POOL_SIZE),
      next_token: 0
    },
    event_loop: EventLoop::new().ok().expect("EventLoop creation failed."),
  }
}

