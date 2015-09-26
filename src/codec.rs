use std::result::Result;

#[derive(Debug)]
pub struct BytesRead(pub usize);
#[derive(Debug)]
pub struct BytesWritten(pub usize);

#[derive(Debug)]
pub enum DecodingError {
  ProtocolError,
  IncompleteFrame
}

#[derive(Debug)]
pub enum EncodingError {
  ProtocolError,
  InsufficientBuffer
}

pub struct DecodedFrame<F> {
  pub frame: F,
  pub bytes_read: BytesRead
}

impl <F> DecodedFrame<F> {
  pub fn new(frame: F, bytes_read: BytesRead) -> DecodedFrame<F> {
    DecodedFrame {
      frame: frame,
      bytes_read: bytes_read
    }
  }
}

// No need for EncodedFrame type yet (ever?)

pub type DecodingResult<F> = Result<DecodedFrame<F>, DecodingError>;
pub type EncodingResult = Result<BytesWritten, EncodingError>;

pub trait Codec<F> : Send {
  fn new() -> Self;
  fn encode(&mut self, frame: &F, &mut [u8]) -> EncodingResult;
  fn decode(&mut self, &[u8]) -> DecodingResult<F>;
}
