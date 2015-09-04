pub trait FrameHandler<F> {
  fn on_frame_received(&mut self, F);
  fn on_frame_written(&mut self, F);
}
