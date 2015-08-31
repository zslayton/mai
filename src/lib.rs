#![allow(dead_code)]

#[macro_use]
extern crate log;
extern crate mio;
extern crate slab;
extern crate lifeguard;

pub mod codec;
pub mod buffer;
pub mod streams;

pub use buffer::Buffer;
pub use streams::FrameEngine;
pub use streams::FrameHandler;
pub use streams::FrameEngineBuilder;
pub use streams::EventedByteStream;

use mio::{EventLoop};
use mio::util::Slab;
use lifeguard::Pool;

use codec::Codec;

use std::marker::PhantomData;

pub fn frame_engine<E, F, C, H>(codec: C, frame_handler: H) -> FrameEngineBuilder<E, F, C, H> where
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<F>
{
  FrameEngineBuilder {
    frame_engine: FrameEngine::new(codec, frame_handler),
    event_loop: EventLoop::new().ok().expect("EventLoop creation failed."),
  }
}

