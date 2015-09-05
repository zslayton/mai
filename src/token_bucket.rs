use mio::Token;
use slab::Index;

pub struct TokenBucket {
  free_tokens: Vec<usize>
}

impl TokenBucket {
  pub fn new() -> TokenBucket {
    TokenBucket {
      free_tokens: Vec::new()
    }
  }

  pub fn get(&mut self) -> Option<Token> {
    self.free_tokens.pop().map(|usize_token| Index::from_usize(usize_token))
  }

  pub fn put(&mut self, token: Token) {
    self.free_tokens.push(token.as_usize());
  }
}
