extern crate mai;

#[cfg(test)]
mod tests {
  use mai::Buffer;

#[test]
  fn test_restack() {
    let mut buffer : Buffer = Buffer::new();
    buffer.extend(&[0u8, 1, 2, 3, 4, 5]);
    println!("Buffer: {:?}", buffer);
    assert_eq!(6, buffer.len());
    buffer.restack(3);
    assert_eq!(3, buffer.len());
    assert_eq!(buffer.bytes(), &[3, 4, 5]);
  }

  #[test]
  fn test_restack_nothing_consumed() {
    let mut buffer : Buffer = Buffer::new();
    buffer.extend(&[0u8, 1, 2, 3, 4, 5]);
    println!("Buffer: {:?}", buffer);
    assert_eq!(6, buffer.len());
    buffer.restack(0);
    assert_eq!(6, buffer.len());
    assert_eq!(buffer.bytes(), &[0, 1, 2, 3, 4, 5]);
  }

  #[test]
  fn test_restack_everything_consumed() {
    let mut buffer : Buffer = Buffer::new();
    buffer.extend(&[0u8, 1, 2, 3, 4, 5]);
    println!("Buffer: {:?}", buffer);
    assert_eq!(6, buffer.len());
    buffer.restack(6);
    assert_eq!(0, buffer.len());
    assert_eq!(buffer.bytes(), &[]);
  }
}
