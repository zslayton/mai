use std::result::Result;

#[derive(Debug)]
pub struct BytesRead(pub usize);
#[derive(Debug)]
pub struct BytesWritten(pub usize);

#[derive(Debug)]
pub enum DecodingError {
  InvalidFrame(BytesRead),
  IncompleteFrame
}

#[derive(Debug)]
pub enum EncodingError {
  InvalidFrame,
  InsufficientBufferSize
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

pub trait Codec<F> {
  fn encode(&mut self, frame: &F, &mut [u8]) -> EncodingResult;
  fn decode(&mut self, &[u8]) -> DecodingResult<F>;
}
