#![allow(dead_code)]

#[macro_use]
extern crate log;
extern crate mio;
extern crate slab;
extern crate lifeguard;

mod evented_frame_stream;
mod frame_engine;
mod protocol;
mod codec;
mod buffer;
mod evented_byte_stream;
mod context;
mod handler;
mod error;
mod remote;
mod token_bucket;
mod stream_manager;
mod settings;
mod stream_id;

pub use stream_id::StreamId;
pub use settings::*;
pub use codec::*;
pub use protocol::Protocol;
pub use error::Error;
pub use buffer::Buffer;
pub use handler::Handler;
pub use context::Context;
pub use remote::FrameEngineRemote;
pub use evented_byte_stream::EventedByteStream;
pub use evented_frame_stream::EventedFrameStream;
pub use evented_frame_stream::StreamState;
pub use evented_frame_stream::Outbox;
pub use frame_engine::FrameEngine;
pub use frame_engine::FrameEngineBuilder;
pub use frame_engine::Command;

use mio::{EventLoop};

pub fn frame_engine<P>(handler: P::Handler) -> frame_engine::FrameEngineBuilder<P> 
  where P: Protocol {
  FrameEngineBuilder {
    handler: handler,
    event_loop: EventLoop::new().ok().expect("EventLoop creation failed."),
    starting_buffer_size: Kilobytes(32).to_bytes(),
    buffer_pool_size: 32,
    max_buffer_pool_size: 32,
  }
}

