use std::collections::BTreeMap;

use slab::Index;
use mio::Token;

use EventedByteStream;
use EventedFrameStream;
use ::token_bucket::TokenBucket;

pub struct StreamManager<E, F> where E: EventedByteStream {
  streams: BTreeMap<usize, EventedFrameStream<E,F>>,
  token_bucket: TokenBucket,
}

impl <E, F> StreamManager <E, F> where E: EventedByteStream {
  pub fn new() -> StreamManager<E, F> {
    StreamManager {
      streams: BTreeMap::new(),
      token_bucket: TokenBucket::new(),
    }
  }

  pub fn get_mut(&mut self, token: Token) -> Option<&mut EventedFrameStream<E, F>> {
    self.streams.get_mut(&token.as_usize())
  }

  fn get_next_token(&mut self) -> Token {
    if let Some(token) = self.token_bucket.get() {
      return token;
    }
    Index::from_usize(self.streams.len())
  }

  pub fn insert(&mut self, efs: EventedFrameStream<E, F>) -> Token {
    let next_token = self.get_next_token();
    self.streams.insert(next_token.as_usize(), efs);
    next_token
  }

  pub fn remove(&mut self, token: Token) -> Option<EventedFrameStream<E,F>> {
    self.token_bucket.put(token);
    self.streams.remove(&token.as_usize())
  }
}
