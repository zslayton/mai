use mio::Token;
use ::error::Error;
use Codec;
use EventedByteStream;
use FrameStream;

pub trait FrameHandler<E, F> : Send where 
  E: EventedByteStream,
  F: Send {
  fn on_ready(&mut self, stream: &mut FrameStream<E,F>) {
    debug!("Stream for {:?} is ready to start reading and writing frames.", stream.token());
  }

  fn on_frame(&mut self, stream: &mut FrameStream<E,F>, _frame: F) {
    debug!("Stream for {:?} received a frame.", stream.token());
  }
  
  fn on_error(&mut self, stream: &mut FrameStream<E,F>, error: Error) { 
    error!("An error occurred on stream for {:?}: {:?}.", stream.token(), error);
  }

  fn on_closed(&mut self, stream: &FrameStream<E,F>) {
    debug!("Stream for {:?} closed.", stream.token());
  }
}
