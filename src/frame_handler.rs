use mio::Token;
use ::error::Error;
use Codec;
use EventedByteStream;
use FrameStream;
use Protocol;

pub trait FrameHandler<P: ?Sized> : Send where P : Protocol {

  fn on_ready(&mut self, stream: &mut FrameStream<P>) {
    debug!("Stream for {:?} is ready to start reading and writing frames.", stream.token());
  }

  fn on_frame(&mut self, stream: &mut FrameStream<P>, _frame: P::Frame) {
    debug!("Stream for {:?} received a frame.", stream.token());
  }
  
  fn on_timeout(&mut self, timeout: P::Timeout) {
    debug!("TIMEOUT!"); 
  }

  fn on_error(&mut self, stream: &mut FrameStream<P>, error: Error) { 
    error!("An error occurred on stream for {:?}: {:?}.", stream.token(), error);
  }

  fn on_closed(&mut self, stream: &FrameStream<P>) {
    debug!("Stream for {:?} closed.", stream.token());
  }
}
