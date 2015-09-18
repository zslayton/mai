#![allow(dead_code)]

#[macro_use]
extern crate log;
extern crate mio;
extern crate slab;
extern crate lifeguard;

mod protocol;
mod codec;
mod buffer;
mod evented_byte_stream;
mod evented_frame_stream;
mod context;
mod handler;
mod error;
mod remote;
mod token_bucket;
mod stream_manager;
mod frame_engine;
mod settings;

pub use settings::*;
pub use protocol::Protocol;
pub use error::Error;
pub use buffer::Buffer;
pub use handler::Handler;
pub use codec::*;
pub use context::Context;
pub use frame_engine::FrameEngineBuilder;
pub use frame_engine::FrameEngine;
pub use frame_engine::Command;
pub use remote::FrameEngineRemote;
pub use evented_byte_stream::EventedByteStream;
pub use evented_frame_stream::EventedFrameStream;
pub use evented_frame_stream::StreamState;
pub use evented_frame_stream::Outbox;

use mio::{EventLoop};

pub fn frame_engine<P>(codec: P::Codec, handler: P::Handler) -> frame_engine::FrameEngineBuilder<P> 
  where P: Protocol {
  FrameEngineBuilder {
    codec: codec,
    handler: handler,
    event_loop: EventLoop::new().ok().expect("EventLoop creation failed."),
    starting_buffer_size: Kilobytes(32).to_bytes(),
    buffer_pool_size: 32,
    max_buffer_pool_size: 32,
  }
}

