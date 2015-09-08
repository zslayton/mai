use mio::Token;
use ::error::Error;

pub trait FrameHandler<F> {
  fn on_ready(&mut self, token: Token) {
    debug!("Stream for {:?} is ready to start reading and writing frames.", token);
  }

  fn on_frame_received(&mut self, token: Token, _frame: F) {
    debug!("Stream for {:?} received a frame.", token);
  }

/*  fn on_frame_written(&mut self, token: Token, _frame: F) {
    debug!("Stream for {:?} sent a frame successfully.", token);
  }*/
  
  fn on_error(&mut self, token: Token, error: Error) { // TODO: Error info
    error!("An error occurred on stream for {:?}.", token);
  }

  fn on_closed(&mut self, token: Token) {
    debug!("Stream for {:?} closed.", token);
  }
}
