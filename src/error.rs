use std::io;
use codec;

#[derive(Debug)]
pub enum Error {
  Io(io::Error),
  Encoding(codec::EncodingError),
  Decoding(codec::DecodingError)
}
