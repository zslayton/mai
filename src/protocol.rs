use EventedByteStream;
use Codec;
use Handler;

use std::fmt::Debug;

pub trait Protocol {
  type ByteStream: EventedByteStream;
  type Frame: Send + Debug;
  type Codec: Codec<Self::Frame>;
  type Handler: Handler<Self>;
  type Timeout: Debug;
  type Session: Send;
}
