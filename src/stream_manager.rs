use std::collections::BTreeMap;

use slab::Index;
use mio::Token;

use Protocol;
use ::token_bucket::TokenBucket;
use stream_session::StreamSession;

pub struct StreamManager<P: ?Sized> where P: Protocol {
  streams: BTreeMap<usize, StreamSession<P>>,
  token_bucket: TokenBucket,
}

impl <P: ?Sized> StreamManager <P> where P: Protocol {
  pub fn new() -> StreamManager<P> {
    StreamManager {
      streams: BTreeMap::new(),
      token_bucket: TokenBucket::new(),
    }
  }

  pub fn get_mut(&mut self, token: Token) -> Option<&mut StreamSession<P>> {
    self.streams.get_mut(&token.as_usize())
  }

  fn get_next_token(&mut self) -> Token {
    if let Some(token) = self.token_bucket.get() {
      return token;
    }
    Index::from_usize(self.streams.len())
  }

  pub fn insert(&mut self, stream_session: StreamSession<P>) -> Token {
    let next_token = self.get_next_token();
    self.streams.insert(next_token.as_usize(), stream_session);
    next_token
  }

  pub fn remove(&mut self, token: Token) -> Option<StreamSession<P>> {
    self.token_bucket.put(token);
    self.streams.remove(&token.as_usize())
  }
}
