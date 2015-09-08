#![allow(dead_code)]

#[macro_use]
extern crate log;
extern crate mio;
extern crate slab;
extern crate lifeguard;

pub mod codec;
pub mod buffer;
pub mod frame_engine;
pub mod evented_byte_stream;
mod evented_frame_stream;
pub mod frame_handler;
pub mod error;

mod token_bucket;
mod stream_manager;

pub use error::Error;
pub use buffer::Buffer;
pub use frame_handler::FrameHandler;
pub use evented_byte_stream::EventedByteStream;
pub use evented_frame_stream::EventedFrameStream;
pub use evented_frame_stream::StreamState;
pub use evented_frame_stream::Outbox;
pub use frame_engine::FrameEngineBuilder;
pub use frame_engine::FrameEngine;
pub use frame_engine::Command;
pub use codec::Codec;

use mio::EventLoop;

pub fn frame_engine<E, F, C, H>(codec: C, frame_handler: H) -> FrameEngineBuilder<E, F, C, H> where
  E: EventedByteStream,
  C: Codec<F>,
  H: FrameHandler<F>,
  F: Send
{
  FrameEngineBuilder {
    frame_engine: FrameEngine::new(codec, frame_handler),
    event_loop: EventLoop::new().ok().expect("EventLoop creation failed."),
  }
}

