use std::{
  borrow::Cow,
  cell::RefCell,
  collections::VecDeque,
  hash::Hash,
  ops::{Bound, RangeBounds},
  rc::Rc,
};

use crate::Error;

#[derive(Clone, Debug)]
enum Repr<'a> {
  Simple(&'a str),
  Complex(Rc<Vec<(&'a str, usize)>>),
}

/// A rope data structure.
#[derive(Clone, Debug)]
pub struct Rope<'a>(Repr<'a>);

impl<'a> Rope<'a> {
  /// Creates a new empty rope.
  pub const fn new() -> Self {
    Self(Repr::Simple(""))
  }

  /// Adds a string slice to the end of the rope.
  ///
  /// Converts from simple to complex representation on first add.
  /// Empty strings are ignored.
  pub fn add(&mut self, value: &'a str) {
    if value.is_empty() {
      return;
    }

    match &mut self.0 {
      Repr::Simple(s) => {
        let vec = Vec::from_iter([(*s, 0), (value, s.len())]);
        self.0 = Repr::Complex(Rc::new(vec));
      }
      Repr::Complex(data) => {
        let len = data
          .last()
          .map_or(0, |(chunk, start_pos)| *start_pos + chunk.len());
        Rc::make_mut(data).push((value, len));
      }
    }
  }

  /// Appends another rope to this rope.
  ///
  /// Handles all combinations of simple and complex representations efficiently.
  pub fn append(&mut self, value: Rope<'a>) {
    match (&mut self.0, value.0) {
      (Repr::Simple(s), Repr::Simple(other)) => {
        let vec = Vec::from_iter([(*s, 0), (other, s.len())]);
        self.0 = Repr::Complex(Rc::new(vec));
      }
      (Repr::Complex(data), Repr::Complex(value_data)) => {
        if !value_data.is_empty() {
          let mut len = data
            .last()
            .map_or(0, |(chunk, start_pos)| *start_pos + chunk.len());

          let cur = Rc::make_mut(data);
          cur.reserve_exact(value_data.len());

          for &(value, _) in value_data.iter() {
            cur.push((value, len));
            len += value.len();
          }
        }
      }
      (Repr::Complex(data), Repr::Simple(other)) => {
        if !other.is_empty() {
          let len = data
            .last()
            .map_or(0, |(chunk, start_pos)| *start_pos + chunk.len());
          Rc::make_mut(data).push((other, len));
        }
      }
      (Repr::Simple(s), Repr::Complex(other_data)) => {
        let mut vec = Vec::with_capacity(other_data.len() + 1);
        vec.push((*s, 0));
        let s_len = s.len();

        for &(value, _) in other_data.iter() {
          vec.push((value, s_len + other_data[0].1));
        }
        self.0 = Repr::Complex(Rc::new(vec));
      }
    }
  }

  /// Gets the byte at the given index.
  ///
  /// # Panics
  /// When index is out of bounds.
  pub fn byte(&self, byte_index: usize) -> u8 {
    self.get_byte(byte_index).expect("byte out of bounds")
  }

  /// Non-panicking version of [Rope::byte].
  ///
  /// Gets the byte at the given index, returning None if out of bounds.
  pub fn get_byte(&self, byte_index: usize) -> Option<u8> {
    if byte_index >= self.len() {
      return None;
    }
    match &self.0 {
      Repr::Simple(s) => Some(s.as_bytes()[byte_index]),
      Repr::Complex(data) => {
        let chunk_index = data
          .binary_search_by(|(_, start_pos)| start_pos.cmp(&byte_index))
          .unwrap_or_else(|index| index.saturating_sub(1));
        let (s, start_pos) = &data.get(chunk_index)?;
        let pos = byte_index - start_pos;
        Some(s.as_bytes()[pos])
      }
    }
  }

  /// Returns an iterator over the characters and their byte positions.
  pub fn char_indices(&self) -> CharIndices<'_, 'a> {
    match &self.0 {
      Repr::Simple(s) => CharIndices {
        repr: CharIndicesRepr::Simple {
          iter: s.char_indices(),
        },
      },
      Repr::Complex(data) => CharIndices {
        repr: CharIndicesRepr::Complex {
          chunks: data,
          char_indices: VecDeque::new(),
          chunk_index: 0,
        },
      },
    }
  }

  /// Returns whether the rope starts with the given string.
  #[inline]
  pub fn starts_with(&self, value: &str) -> bool {
    match &self.0 {
      Repr::Simple(s) => s.starts_with(value),
      Repr::Complex(data) => {
        if let Some((first, _)) = data.first() {
          first.starts_with(value)
        } else {
          false
        }
      }
    }
  }

  /// Returns whether the rope ends with the given string.
  #[inline]
  pub fn ends_with(&self, value: &str) -> bool {
    match &self.0 {
      Repr::Simple(s) => s.ends_with(value),
      Repr::Complex(data) => {
        if let Some((last, _)) = data.last() {
          last.ends_with(value)
        } else {
          false
        }
      }
    }
  }

  /// Returns whether the rope is empty.
  #[inline]
  pub fn is_empty(&self) -> bool {
    match &self.0 {
      Repr::Simple(s) => s.is_empty(),
      Repr::Complex(data) => data.iter().all(|(s, _)| s.is_empty()),
    }
  }

  /// Returns the length of the rope in bytes.
  #[inline]
  pub fn len(&self) -> usize {
    match &self.0 {
      Repr::Simple(s) => s.len(),
      Repr::Complex(data) => data
        .last()
        .map_or(0, |(chunk, start_pos)| start_pos + chunk.len()),
    }
  }

  /// Returns a slice of the rope in the given byte range.
  ///
  /// # Panics
  /// - When start > end
  /// - When end is out of bounds
  /// - When indices are not on char boundaries
  pub fn byte_slice<R>(&self, range: R) -> Rope<'a>
  where
    R: RangeBounds<usize>,
  {
    self.get_byte_slice_impl(range).unwrap_or_else(|e| {
      panic!("byte_slice: {}", e);
    })
  }

  /// Non-panicking version of [Rope::byte_slice].
  pub fn get_byte_slice<R>(&self, range: R) -> Option<Rope<'a>>
  where
    R: RangeBounds<usize>,
  {
    self.get_byte_slice_impl(range).ok()
  }

  /// Implementation for byte_slice operations.
  #[inline]
  pub(crate) fn get_byte_slice_impl<R>(
    &self,
    range: R,
  ) -> Result<Rope<'a>, Error>
  where
    R: RangeBounds<usize>,
  {
    let start_range = start_bound_to_range_start(range.start_bound());
    let end_range = end_bound_to_range_end(range.end_bound());

    match (start_range, end_range) {
      (Some(start), Some(end)) => {
        if start > end {
          return Err(Error::Rope("start >= end"));
        } else if end > self.len() {
          return Err(Error::Rope("end out of bounds"));
        }
      }
      (None, Some(end)) => {
        if end > self.len() {
          return Err(Error::Rope("end out of bounds"));
        }
      }
      (Some(start), None) => {
        if start > self.len() {
          return Err(Error::Rope("start out of bounds"));
        }
      }
      _ => {}
    }

    let start_range = start_range.unwrap_or(0);
    let end_range = end_range.unwrap_or_else(|| self.len());

    match &self.0 {
      Repr::Simple(s) => s
        .get(start_range..end_range)
        .map(Rope::from)
        .ok_or_else(|| Error::Rope("invalid char boundary")),
      Repr::Complex(data) => {
        // [start_chunk
        let start_chunk_index = data
          .binary_search_by(|(_, start_pos)| start_pos.cmp(&start_range))
          .unwrap_or_else(|insert_pos| insert_pos.saturating_sub(1));

        // end_chunk)
        let end_chunk_index = data
          .binary_search_by(|(chunk, start_pos)| {
            let end_pos = start_pos + chunk.len(); // exclusive
            end_pos.cmp(&end_range)
          })
          .unwrap_or_else(|insert_pos| insert_pos);

        // same chunk
        if start_chunk_index == end_chunk_index {
          let (chunk, start_pos) = data[start_chunk_index];
          let start = start_range - start_pos;
          let end = end_range - start_pos;
          return chunk
            .get(start..end)
            .map(Rope::from)
            .ok_or_else(|| Error::Rope("invalid char boundary"));
        }

        if end_chunk_index < start_chunk_index {
          return Ok(Rope::new());
        }

        let mut raw =
          Vec::with_capacity(end_chunk_index - start_chunk_index + 1);
        let mut len = 0;

        // different chunk
        // [start_chunk, end_chunk]
        (start_chunk_index..end_chunk_index + 1).try_for_each(|i| {
          let (chunk, start_pos) = data[i];

          if start_chunk_index == i {
            let start = start_range - start_pos;
            if let Some(chunk) = chunk.get(start..) {
              raw.push((chunk, len));
              len += chunk.len();
            } else {
              return Err(Error::Rope("invalid char boundary"));
            }
          } else if end_chunk_index == i {
            let end = end_range - start_pos;
            if let Some(chunk) = chunk.get(..end) {
              raw.push((chunk, len));
              len += chunk.len();
            } else {
              return Err(Error::Rope("invalid char boundary"));
            }
          } else {
            raw.push((chunk, len));
            len += chunk.len();
          }

          Ok(())
        })?;

        Ok(Rope(Repr::Complex(Rc::new(raw))))
      }
    }
  }

  /// Converts the rope to bytes.
  ///
  /// Returns borrowed bytes for simple ropes and owned bytes for complex ropes.
  pub fn to_bytes(&self) -> Cow<'a, [u8]> {
    match &self.0 {
      Repr::Simple(s) => Cow::Borrowed(s.as_bytes()),
      Repr::Complex(data) => {
        let mut bytes = vec![];
        for (chunk, _) in data.iter() {
          bytes.extend_from_slice(chunk.as_bytes());
        }
        Cow::Owned(bytes)
      }
    }
  }

  /// Returns the underlying str if this is a simple rope.
  pub fn get_simple(&self) -> Option<&'a str> {
    match &self.0 {
      Repr::Simple(s) => Some(s),
      _ => None,
    }
  }
}

impl Hash for Rope<'_> {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    match &self.0 {
      Repr::Simple(s) => s.hash(state),
      Repr::Complex(data) => {
        for (s, _) in data.iter() {
          s.hash(state);
        }
      }
    }
  }
}

enum CharIndicesRepr<'a, 'b> {
  Simple {
    iter: std::str::CharIndices<'b>,
  },
  Complex {
    chunks: &'a [(&'b str, usize)],
    char_indices: VecDeque<(usize, char)>,
    chunk_index: usize,
  },
}

pub struct CharIndices<'a, 'b> {
  repr: CharIndicesRepr<'a, 'b>,
}

impl Iterator for CharIndices<'_, '_> {
  type Item = (usize, char);

  fn next(&mut self) -> Option<Self::Item> {
    match &mut self.repr {
      CharIndicesRepr::Simple { iter } => iter.next(),
      CharIndicesRepr::Complex {
        chunks,
        char_indices,
        chunk_index,
      } => {
        if let Some(item) = char_indices.pop_front() {
          return Some(item);
        }

        if *chunk_index >= chunks.len() {
          return None;
        }

        // skip empty chunks
        while *chunk_index < chunks.len() && chunks[*chunk_index].0.is_empty() {
          *chunk_index += 1;
        }

        let (chunk, start_pos) = chunks[*chunk_index];

        char_indices
          .extend(chunk.char_indices().map(|(i, c)| (start_pos + i, c)));
        *chunk_index += 1;
        char_indices.pop_front()
      }
    }
  }
}

impl Default for Rope<'_> {
  fn default() -> Self {
    Self::new()
  }
}

impl ToString for Rope<'_> {
  fn to_string(&self) -> String {
    match &self.0 {
      Repr::Simple(s) => s.to_string(),
      Repr::Complex(data) => {
        let mut s = String::with_capacity(self.len());
        for (chunk, _) in data.iter() {
          s.push_str(chunk);
        }
        s
      }
    }
  }
}

impl PartialEq<Rope<'_>> for Rope<'_> {
  fn eq(&self, other: &Rope<'_>) -> bool {
    if self.len() != other.len() {
      return false;
    }

    if self.is_empty() {
      return true;
    }

    let chunks = match &self.0 {
      Repr::Simple(s) => &[(*s, 0)][..],
      Repr::Complex(data) => &data[..],
    };
    let other_chunks = match &other.0 {
      Repr::Simple(s) => &[(*s, 0)][..],
      Repr::Complex(data) => &data[..],
    };

    let mut cur = 0;
    let other_chunk_index = RefCell::new(0);
    let mut other_chunk_byte_index = 0;
    let other_chunk = || other_chunks[*other_chunk_index.borrow()].0.as_bytes();
    for (chunk, start_pos) in chunks.iter() {
      let chunk = chunk.as_bytes();
      while (cur - start_pos) < chunk.len() {
        if other_chunk_byte_index >= other_chunk().len() {
          other_chunk_byte_index = 0;
          *other_chunk_index.borrow_mut() += 1;
        }
        if chunk[cur - start_pos] == other_chunk()[other_chunk_byte_index] {
          cur += 1;
          other_chunk_byte_index += 1;
        } else {
          return false;
        }
      }
    }

    true
  }
}

impl PartialEq<str> for Rope<'_> {
  fn eq(&self, other: &str) -> bool {
    if self.len() != other.len() {
      return false;
    }

    let other = other.as_bytes();

    match &self.0 {
      Repr::Simple(s) => {
        if s.as_bytes() != other {
          return false;
        }
      }
      Repr::Complex(data) => {
        let mut idx = 0;
        for (chunk, _) in data.iter() {
          let chunk = chunk.as_bytes();
          if chunk != &other[idx..(idx + chunk.len())] {
            return false;
          }
          idx += chunk.len();
        }
      }
    }

    true
  }
}

impl PartialEq<&str> for Rope<'_> {
  fn eq(&self, other: &&str) -> bool {
    if self.len() != other.len() {
      return false;
    }

    let other = other.as_bytes();

    match &self.0 {
      Repr::Simple(s) => {
        if s.as_bytes() != other {
          return false;
        }
      }
      Repr::Complex(data) => {
        let mut idx = 0;
        for (chunk, _) in data.iter() {
          let chunk = chunk.as_bytes();
          if chunk != &other[idx..(idx + chunk.len())] {
            return false;
          }
          idx += chunk.len();
        }
      }
    }

    true
  }
}

impl<'a> From<&'a str> for Rope<'a> {
  fn from(value: &'a str) -> Self {
    Rope(Repr::Simple(value))
  }
}

impl<'a> From<&'a String> for Rope<'a> {
  fn from(value: &'a String) -> Self {
    Rope(Repr::Simple(value))
  }
}

impl<'a> From<&'a Cow<'a, str>> for Rope<'a> {
  fn from(value: &'a Cow<'a, str>) -> Self {
    Rope(Repr::Simple(value))
  }
}

#[inline(always)]
fn start_bound_to_range_start(start: Bound<&usize>) -> Option<usize> {
  match start {
    Bound::Included(&start) => Some(start),
    Bound::Excluded(&start) => Some(start + 1),
    Bound::Unbounded => None,
  }
}

#[inline(always)]
fn end_bound_to_range_end(end: Bound<&usize>) -> Option<usize> {
  match end {
    Bound::Included(&end) => Some(end + 1),
    Bound::Excluded(&end) => Some(end),
    Bound::Unbounded => None,
  }
}

#[cfg(test)]
mod tests {
  use crate::rope::Rope;

  #[test]
  fn add() {
    let mut r = Rope::new();
    r.add("a");
    r.add("b");
    assert_eq!(r.to_string(), "ab".to_string());
  }

  #[test]
  fn slice() {
    let mut a = Rope::new();
    a.add("abc");
    a.add("def");
    a.add("ghi");

    // same chunk start
    let rope = a.byte_slice(0..1);
    assert_eq!(rope.to_string(), "a".to_string());

    // same chunk end
    let rope = a.byte_slice(2..3);
    assert_eq!(rope.to_string(), "c".to_string());

    // cross chunks
    let rope = a.byte_slice(2..5);
    assert_eq!(rope.to_string(), "cde".to_string());

    // empty slice
    let rope = a.byte_slice(0..0);
    assert_eq!(rope.to_string(), "".to_string());

    // slice with len
    let rope = Rope::from("abc");
    let rope = rope.byte_slice(3..3);
    assert_eq!(rope.to_string(), "".to_string())
  }

  #[test]
  #[should_panic]
  fn slice_panics_range_start_out_of_bounds() {
    let mut a = Rope::new();
    a.add("abc");
    a.byte_slice(3..4);
  }

  #[test]
  #[should_panic]
  fn slice_panics_range_start_greater_than_end() {
    let mut a = Rope::new();
    a.add("abc");
    a.byte_slice(1..0);
  }

  #[test]
  #[should_panic]
  fn slice_panics_range_end_out_of_bounds() {
    let mut a = Rope::new();
    a.add("abc");
    a.byte_slice(0..4);
  }

  #[test]
  fn eq() {
    let mut a = Rope::new();
    a.add("abc");
    a.add("def");
    a.add("ghi");
    assert_eq!(&a, "abcdefghi");
    assert_eq!(a, "abcdefghi");

    let mut b = Rope::new();
    b.add("abcde");
    b.add("fghi");

    assert_eq!(a, b);
  }

  #[test]
  fn from() {
    let _ = Rope::from("abc");
    let _ = Rope::from("abc");
  }

  #[test]
  fn byte() {
    let mut a = Rope::from("abc");
    assert_eq!(a.byte(0), b'a');
    a.add("d");
    assert_eq!(a.byte(3), b'd');
  }

  #[test]
  fn char_indices() {
    let mut a = Rope::new();
    a.add("abc");
    a.add("def");

    let a = a.char_indices().collect::<Vec<_>>();
    let b = "abcdef".char_indices().collect::<Vec<_>>();

    assert_eq!(a, b);
  }
}
