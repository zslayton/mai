use lifeguard::{Recycleable};
use std::fmt;

const DEFAULT_BUFFER_SIZE : usize = 16 * 1_024;

pub struct Buffer {
  bytes: Box<[u8]>,
  in_use: usize
}

impl fmt::Debug for Buffer {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "Buffer{{len={:?}, bytes={:?}}}", self.len(), self.bytes())
  }
}

impl Recycleable for Buffer {
  fn new() -> Buffer {
    Buffer::new()
  }

  fn reset(&mut self) {
    //self.bytes.reset();
    self.reset();
  }
}

impl Buffer {
  pub fn new() -> Buffer {
    Buffer::with_capacity(DEFAULT_BUFFER_SIZE)
  }

  pub fn with_capacity(capacity: usize) -> Buffer {
    let bytes = (0..capacity)
        .map(|_| 0u8)
        .collect::<Vec<_>>()
        .into_boxed_slice();
    Buffer {
      bytes: bytes, 
      in_use: 0
    }
  }

  pub fn reset(&mut self) {
    debug!("Resetting buffer, clearing {} bytes", self.in_use);
    self.in_use = 0;
  }

  pub fn push(&mut self, byte: u8) {
    self.bytes[self.in_use] = byte;
    self.in_use += 1;
  }

  pub fn extend(&mut self, bytes: &[u8]) {
    for &byte in bytes {
      self.push(byte)
    }
  }

  pub fn bytes(&self) -> &[u8] {
    &self.bytes[0..self.in_use]
  }

  pub fn remaining(&mut self) -> &mut [u8] {
    &mut self.bytes[self.in_use..]
  }

  pub fn set_size(&mut self, size: usize) {
    self.in_use = size;
  }

  pub fn truncate(&mut self, size: usize) {
    use std::cmp;
    if size > self.len() {
      panic!("Cannot truncate to {} bytes. Buffer is only {} bytes long.", size, self.len());
    }
    self.in_use = cmp::min(size, self.len());
  }

  pub fn len(&self) -> usize {
    self.in_use 
  }

  pub fn restack(&mut self, num_consumed : usize) {
    debug!("Restacking buffer");
    let num_in_use = self.len();
    // If nothing was consumed, we're done
    if num_consumed == 0 {
      return;
    }
    // If everything was consumed, empty the buffer and return
    if num_consumed == num_in_use {
      self.reset();
      return;
    }
    // Otherwise, shift the remaining bytes as far left as possible
    for i in num_consumed..num_in_use {
      let (left, right) = self.bytes.split_at_mut(i);
      println!("i={}, left={:?}, right={:?}", i, left, right);
      left[i-num_consumed] = right[0];
    }
    self.truncate(num_in_use - num_consumed);
  }
}

