use std::result::Result;

pub struct BytesConsumed(usize);
pub struct BytesProduced(usize);

pub enum DecodingError {
  InvalidFrame(BytesConsumed),
  IncompleteFrame
}

pub enum EncodingError {
  InvalidFrame,
  InsufficientBufferSize
}

pub struct DecodedFrame<F> {
  frame: F,
  bytes_consumed: BytesConsumed
}

pub struct EncodedFrame<F> {
  frame: F,
  bytes_produced: BytesProduced
}

pub type DecodingResult<F> = Result<Option<DecodedFrame<F>>, DecodingError>;
pub type EncodingResult<F> = Result<Option<EncodedFrame<F>>, EncodingError>;

pub trait Codec<F> {
  fn encode(&mut self, &mut [u8]) -> EncodingResult<F>;
  fn decode(&mut self, &[u8]) -> DecodingResult<F>;
}
