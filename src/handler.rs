use ::error::Error;
use Context;
use context::EngineHandle;
use Protocol;

pub trait Handler<P: ?Sized> where P : Protocol {

  fn on_ready(&mut self, context: &mut Context<P>) {
    debug!("Stream for {:?} is ready to start reading and writing frames.", context.stream().token());
  }

  fn on_frame(&mut self, context: &mut Context<P>, _frame: P::Frame) {
    debug!("Stream for {:?} received a frame.", context.stream().token());
  }

  fn on_timeout(&mut self, context: &mut Context<P>, timeout: P::Timeout) {
    debug!("A timeout ({:?}) occurred on stream {:?}", timeout, context.stream().token());
  }

  #[allow(unused_variables)]
  fn on_global_timeout(&mut self, engine: EngineHandle<P>, timeout: P::Timeout) {
    debug!("A global timeout occurred: {:?}", timeout);
  }

  fn on_error(&mut self, context: &mut Context<P>, error: &Error) {
    error!("An error occurred on context for {:?}: {:?}.", context.stream().token(), error);
  }

  fn on_closed(&mut self, context: &mut Context<P>) {
    debug!("Stream for {:?} closed.", context.stream().token());
  }
}
