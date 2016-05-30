#![allow(dead_code)]

#[macro_use]
extern crate log;
extern crate mio;
extern crate slab;
extern crate lifeguard;

mod evented_frame_stream;
mod protocol_engine;
mod protocol;
mod codec;
mod buffer;
mod evented_byte_stream;
mod context;
mod handler;
mod error;
mod remote;
mod token_bucket;
mod timeout;
mod stream_manager;
mod settings;
mod stream_id;
mod stream_session;

pub use stream_id::StreamId;
pub use settings::*;
pub use codec::*;
pub use protocol::Protocol;
pub use error::Error;
pub use buffer::Buffer;
pub use handler::Handler;
pub use context::EngineHandle;
pub use context::StreamHandle;
pub use context::Context;
pub use remote::ProtocolEngineRemote;
pub use evented_byte_stream::EventedByteStream;
pub use evented_frame_stream::EventedFrameStream;
pub use evented_frame_stream::StreamState;
pub use evented_frame_stream::Outbox;
pub use protocol_engine::ProtocolEngine;
pub use protocol_engine::ProtocolEngineBuilder;
pub use protocol_engine::Command;

use mio::{EventLoop};

pub fn protocol_engine<P>(handler: P::Handler) -> protocol_engine::ProtocolEngineBuilder<P>
  where P: Protocol {
  ProtocolEngineBuilder {
    handler: handler,
    event_loop: EventLoop::new().ok().expect("EventLoop creation failed."),
    starting_buffer_size: Kilobytes(32).to_bytes(),
    buffer_pool_size: 32,
    max_buffer_pool_size: 32,
  }
}
