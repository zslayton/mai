use FrameEngineBuilder;
use Protocol;

pub trait OptionSetter<T> {
  fn set_option(self, T) -> T;
}

#[derive(Clone,Copy,Debug)]
pub struct Bytes(pub usize);
#[derive(Clone,Copy,Debug)]
pub struct Kilobytes(pub usize);
#[derive(Clone,Copy,Debug)]
pub struct Megabytes(pub usize);

pub trait ToBytes {
  fn to_bytes(&self) -> Bytes;
}

impl ToBytes for Bytes {
  fn to_bytes(&self) -> Bytes {
    *self
  }
}

impl ToBytes for Kilobytes {
  fn to_bytes(&self) -> Bytes {
    let Kilobytes(kb) = *self;
    Bytes(kb * 1_000)
  }
}

impl ToBytes for Megabytes {
  fn to_bytes(&self) -> Bytes {
    let Megabytes(mb) = *self;
    Bytes(mb * 1_000_000)
  }
}

pub struct InitialBufferSize<T>(T) where T: ToBytes;
pub struct InitialBufferPoolSize(usize);
pub struct MaxBufferPoolSize(usize);

impl <P, T> OptionSetter<FrameEngineBuilder<P>> for InitialBufferSize<T> where P: Protocol, T: ToBytes {
  fn set_option(self, mut builder: FrameEngineBuilder<P>) -> FrameEngineBuilder<P> {
    let InitialBufferSize(size) = self;
    let number_of_bytes: Bytes = size.to_bytes();
    builder.starting_buffer_size = number_of_bytes;
    builder
  }
}

impl <P> OptionSetter<FrameEngineBuilder<P>> for InitialBufferPoolSize where P: Protocol {
  fn set_option(self, mut builder: FrameEngineBuilder<P>) -> FrameEngineBuilder<P> {
    let InitialBufferPoolSize(number_of_buffers) = self;
    builder.buffer_pool_size = number_of_buffers;
    builder
  }
}

impl <P> OptionSetter<FrameEngineBuilder<P>> for MaxBufferPoolSize where P: Protocol {
  fn set_option(self, mut builder: FrameEngineBuilder<P>) -> FrameEngineBuilder<P> {
    let MaxBufferPoolSize(number_of_buffers) = self;
    builder.max_buffer_pool_size = number_of_buffers;
    builder
  }
}
