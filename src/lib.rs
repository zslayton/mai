#![allow(dead_code)]

#[macro_use]
extern crate log;
extern crate mio;
extern crate slab;
extern crate lifeguard;

pub mod protocol;
pub mod codec;
pub mod buffer;
pub mod frame_engine;
pub mod evented_byte_stream;
mod evented_frame_stream;
pub mod context;
pub mod handler;
pub mod error;
pub mod remote;

mod token_bucket;
mod stream_manager;

pub use protocol::Protocol;
pub use remote::FrameEngineRemote;
pub use error::Error;
pub use buffer::Buffer;
pub use handler::Handler;
pub use evented_byte_stream::EventedByteStream;
pub use evented_frame_stream::EventedFrameStream;
pub use evented_frame_stream::StreamState;
pub use evented_frame_stream::Outbox;
pub use context::Context;
pub use frame_engine::FrameEngineBuilder;
pub use frame_engine::FrameEngine;
pub use frame_engine::Command;
pub use codec::*;

use mio::{EventLoop};

pub fn frame_engine<P>(codec: P::Codec, handler: P::Handler) -> FrameEngineBuilder<P> 
  where P: Protocol {
  FrameEngineBuilder {
    codec: codec,
    handler: handler,
    event_loop: EventLoop::new().ok().expect("EventLoop creation failed.")
  }
}

