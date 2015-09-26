use mio::Token;

#[derive(Clone,Copy,PartialEq,Eq)]
pub struct StreamId {
  id: u32,
  token: Token
}

impl StreamId {
  pub fn new(id: u32, token: Token) -> StreamId {
    StreamId {
      id: id,
      token: token
    }
  }

  pub fn token(&self) -> Token {
    self.token
  }
}
