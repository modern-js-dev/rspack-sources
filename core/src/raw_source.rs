use std::borrow::Cow;

use crate::{MapOptions, Source, SourceMap};

pub enum RawSource {
  Buffer(Vec<u8>),
  Source(String),
}

impl From<Vec<u8>> for RawSource {
  fn from(i: Vec<u8>) -> Self {
    Self::Buffer(i)
  }
}

impl From<String> for RawSource {
  fn from(i: String) -> Self {
    Self::Source(i)
  }
}

impl RawSource {
  pub fn is_buffer(&self) -> bool {
    matches!(self, Self::Buffer(_))
  }
}

impl Source for RawSource {
  fn map(&self, _: &MapOptions) -> Option<SourceMap> {
    None
  }

  fn buffer(&self) -> &[u8] {
    match self {
      Self::Buffer(i) => i,
      Self::Source(i) => i.as_bytes(),
    }
  }

  fn source(&self) -> Cow<'_, str> {
    match self {
      Self::Buffer(i) => String::from_utf8_lossy(i),
      Self::Source(i) => Cow::Borrowed(i),
    }
  }

  fn size(&self) -> usize {
    match self {
      Self::Buffer(i) => i.len(),
      Self::Source(i) => i.len(),
    }
  }
}
