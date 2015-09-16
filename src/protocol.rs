use EventedByteStream;
use Codec;
use Handler;

use std::fmt::Debug;
use std::default::Default;

pub trait Protocol {
  type ByteStream: EventedByteStream;
  type Frame: Send + Debug;
  type Codec: Codec<Self::Frame>;
  type Handler: Handler<Self>;
  type Timeout: Send + Debug;
  type EngineSession : Default;
  type Session: Default;
}
