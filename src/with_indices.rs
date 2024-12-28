use std::{cell::RefCell, marker::PhantomData};

use crate::helpers::SourceText;

#[derive(Debug, Clone)]
pub struct WithIndices<'a, S>
where
  S: SourceText<'a>,
{
  /// line is a string reference
  pub line: S,
  last_char_index_to_byte_index: RefCell<(u32, u32)>,
  data: PhantomData<&'a S>,
}

impl<'a, S> WithIndices<'a, S>
where
  S: SourceText<'a>,
{
  pub fn new(line: S) -> Self {
    Self {
      line,
      last_char_index_to_byte_index: RefCell::new((0, 0)),
      data: PhantomData,
    }
  }

  pub(crate) fn substring(
    &self,
    start_char_index: usize,
    end_char_index: usize,
  ) -> S {
    if end_char_index <= start_char_index {
      return S::default();
    }

    let line_len = self.line.len();

    let mut start_byte_index =
      if start_char_index == 0 { Some(0) } else { None };
    let mut end_byte_index = if end_char_index == usize::MAX {
      Some(line_len)
    } else {
      None
    };

    let (last_char_index, last_byte_index) =
      *self.last_char_index_to_byte_index.borrow();
    let mut byte_index = last_byte_index as usize;
    let mut char_index = last_char_index as usize;

    if start_char_index >= last_char_index as usize
      || end_char_index >= last_char_index as usize
    {
      for char in self.line.byte_slice(byte_index..line_len).chars() {
        if start_byte_index.is_some() && end_byte_index.is_some() {
          break;
        }
        if char_index == start_char_index {
          start_byte_index = Some(byte_index);
          *self.last_char_index_to_byte_index.borrow_mut() =
            (char_index as u32, byte_index as u32);
        }
        if char_index == end_char_index {
          end_byte_index = Some(byte_index);
          *self.last_char_index_to_byte_index.borrow_mut() =
            (char_index as u32, byte_index as u32);
        }
        byte_index += char.len_utf8();
        char_index += 1;
      }
    } else {
      for char in self.line.byte_slice(0..byte_index).chars().rev() {
        if start_byte_index.is_some() && end_byte_index.is_some() {
          break;
        }
        byte_index -= char.len_utf8();
        char_index -= 1;
        if char_index == end_char_index {
          end_byte_index = Some(byte_index);
          *self.last_char_index_to_byte_index.borrow_mut() =
            (char_index as u32, byte_index as u32);
        }
        if char_index == start_char_index {
          start_byte_index = Some(byte_index);
        }
      }
    }

    let start_byte_index = start_byte_index.unwrap_or(line_len);
    let end_byte_index = end_byte_index.unwrap_or(line_len);

    #[allow(unsafe_code)]
    unsafe {
      // SAFETY: Since `indices` iterates over the `CharIndices` of `self`, we can guarantee
      // that the indices obtained from it will always be within the bounds of `self` and they
      // will always lie on UTF-8 sequence boundaries.
      self
        .line
        .byte_slice_unchecked(start_byte_index..end_byte_index)
    }
  }
}

/// tests are just copy from `substring` crate
#[cfg(test)]
mod tests {
  use crate::Rope;

  use super::WithIndices;
  #[test]
  fn test_substring() {
    assert_eq!(
      WithIndices::new(Rope::from("foobar")).substring(0, 3),
      "foo"
    );
  }

  #[test]
  fn test_out_of_bounds() {
    assert_eq!(
      WithIndices::new(Rope::from("foobar")).substring(0, 10),
      "foobar"
    );
    assert_eq!(WithIndices::new(Rope::from("foobar")).substring(6, 10), "");
  }

  #[test]
  fn test_start_less_than_end() {
    assert_eq!(WithIndices::new(Rope::from("foobar")).substring(3, 2), "");
  }

  #[test]
  fn test_start_and_end_equal() {
    assert_eq!(WithIndices::new(Rope::from("foobar")).substring(3, 3), "");
  }

  #[test]
  fn test_multiple_byte_characters() {
    assert_eq!(
      WithIndices::new(Rope::from("fõøbα®")).substring(2, 5),
      "øbα"
    );
  }

  #[test]
  fn test_last_char_index_to_byte_index() {
    let rope_with_indices = WithIndices::new(Rope::from("foobar"));
    assert_eq!(rope_with_indices.substring(0, 3), "foo");
    assert_eq!(rope_with_indices.substring(3, 5), "ba");
    assert_eq!(rope_with_indices.substring(0, 3), "foo");
  }
}
