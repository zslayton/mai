use mio::Token;
use std::fmt::Debug;

#[derive(Debug)]
pub struct Timeout<T> where T: Debug {
    pub token: Option<Token>,
    pub data: T
}

impl <T> Timeout<T> where T: Debug {
  pub fn global(data: T) -> Timeout<T> {
      Timeout {
          token: None,
          data: data
      }
  }

  pub fn for_stream(token: Token, data: T) -> Timeout<T> {
      Timeout {
          token: Some(token),
          data: data
      }
  }
}
