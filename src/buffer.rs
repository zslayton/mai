use mio::{Evented, EventLoop, Handler};
use mio::util::Slab;

use lifeguard::{Pool, Recycleable, Recycled};

const BUFFER_SIZE : usize = 16 * 1_024;

#[derive(Debug)]
pub struct Buffer {
  bytes: Vec<u8>
}

impl Recycleable for Buffer {
  fn new() -> Buffer {
    Buffer::new()
  }

  fn reset(&mut self) {
    self.bytes.reset();
  }
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

