#![allow(dead_code)]

extern crate mio;
extern crate lifeguard;

use std::result::Result;

use mio::{Evented, EventLoop, Handler};
use mio::util::Slab;

use lifeguard::{Pool, Recycleable, Recycled};

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

const BUFFER_SIZE : usize = 16 * 1_024;

#[derive(Debug)]
struct Buffer {
  bytes: Vec<u8>
}

impl Buffer {
  pub fn new() -> Buffer {
    Buffer {
      bytes: Vec::with_capacity(BUFFER_SIZE)
    }
  }

  pub fn bytes(&self) -> &[u8] {
    &self.bytes
  }

  pub fn len(&self) -> usize {
    self.bytes.len()
  }

  pub fn restack(&mut self, num_consumed : usize) {
    let num_in_use = self.len();
    // If nothing was consumed, we're done
    if num_consumed == 0 {
      return;
    }
    // If everything was consumed, empty the buffer and return
    if num_consumed == num_in_use {
      self.bytes.reset();
      return;
    }
    // Otherwise, shift the remaining bytes as far left as possible
    // [ 0, 1, 2, 3, 4, 5 ]
    println!("num_in_use  : {}", num_in_use);
    println!("num_consumed: {}", num_consumed);
    for i in num_consumed..num_in_use {
      let (left, right) = self.bytes.split_at_mut(i);
      println!("i={}, left={:?}, right={:?}", i, left, right);
      left[i-num_consumed] = right[0];
    }
    self.bytes.truncate(num_in_use - num_consumed);
  }

  pub fn extend(&mut self, bytes: &[u8]){
    self.bytes.extend(bytes)
  }
}

impl Recycleable for Buffer {
  fn new() -> Buffer {
    Buffer::new()
  }

  fn reset(&mut self) {
    self.bytes.reset();
  }
}

pub struct ByteStream<E> where E: Evented {
  stream: E,
  buffer: Option<Buffer>
}

pub struct IoHandler {
  dummy: usize 
}

impl Handler for IoHandler {
  type Timeout = ();
  type Message = (); 
}

const NUMBER_OF_POOLED_BUFFERS: usize = 16 * 1_024;

pub struct StreamManager<E> where E: Evented {
  streams: Slab<E>,
  event_loop: EventLoop<IoHandler>,
  buffer_pool: Pool<Buffer>
}

impl <E> StreamManager<E> where E: Evented {
  
}

#[test]
fn test_restack() {
  let mut buffer : Buffer = Buffer::new();
  buffer.extend(&[0u8, 1, 2, 3, 4, 5]);
  println!("Buffer: {:?}", buffer);
  assert_eq!(6, buffer.len());
  buffer.restack(3);
  assert_eq!(3, buffer.len());
  assert_eq!(buffer.bytes(), &[3, 4, 5]);
}

#[test]
fn test_restack_nothing_consumed() {
  let mut buffer : Buffer = Buffer::new();
  buffer.extend(&[0u8, 1, 2, 3, 4, 5]);
  println!("Buffer: {:?}", buffer);
  assert_eq!(6, buffer.len());
  buffer.restack(0);
  assert_eq!(6, buffer.len());
  assert_eq!(buffer.bytes(), &[0, 1, 2, 3, 4, 5]);
}

#[test]
fn test_restack_everything_consumed() {
  let mut buffer : Buffer = Buffer::new();
  buffer.extend(&[0u8, 1, 2, 3, 4, 5]);
  println!("Buffer: {:?}", buffer);
  assert_eq!(6, buffer.len());
  buffer.restack(6);
  assert_eq!(0, buffer.len());
  assert_eq!(buffer.bytes(), &[]);
}
