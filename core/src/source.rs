use std::{borrow::Cow, convert::TryFrom, fmt, sync::Arc};

use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MapOptions {
  /// If set to false the implementation may omit mappings for columns. (default: true)
  pub columns: bool,
  pub include_source_contents: bool,
  pub file: Option<String>,
}

impl Default for MapOptions {
  fn default() -> Self {
    Self {
      columns: true,
      include_source_contents: true,
      file: Default::default(),
    }
  }
}

pub trait Source {
  fn map(&self, options: &MapOptions) -> Option<SourceMap>;

  fn buffer(&self) -> &[u8];

  fn source(&self) -> Cow<str> {
    String::from_utf8_lossy(self.buffer())
  }

  fn size(&self) -> usize {
    self.buffer().len()
  }
}

pub type BoxSource = Box<dyn Source>;

impl<S: Source + 'static> From<S> for BoxSource {
  fn from(s: S) -> Self {
    Box::new(s)
  }
}

impl Source for &str {
  fn map(&self, _: &MapOptions) -> Option<SourceMap> {
    None
  }

  fn buffer(&self) -> &[u8] {
    self.as_bytes()
  }

  fn source(&self) -> Cow<'_, str> {
    Cow::Borrowed(self)
  }

  fn size(&self) -> usize {
    self.len()
  }
}

impl Source for String {
  fn map(&self, _: &MapOptions) -> Option<SourceMap> {
    None
  }

  fn buffer(&self) -> &[u8] {
    self.as_bytes()
  }

  fn source(&self) -> Cow<'_, str> {
    Cow::Borrowed(self)
  }

  fn size(&self) -> usize {
    self.len()
  }
}

impl Source for &[u8] {
  fn map(&self, _: &MapOptions) -> Option<SourceMap> {
    None
  }

  fn buffer(&self) -> &[u8] {
    self
  }

  fn size(&self) -> usize {
    self.len()
  }
}

impl Source for Vec<u8> {
  fn map(&self, _: &MapOptions) -> Option<SourceMap> {
    None
  }

  fn buffer(&self) -> &[u8] {
    self
  }

  fn source(&self) -> Cow<'_, str> {
    String::from_utf8_lossy(self)
  }

  fn size(&self) -> usize {
    self.len()
  }
}

#[derive(Debug, Clone)]
pub struct SourceMap(Arc<RwLock<sourcemap::SourceMap>>);

impl SourceMap {
  pub fn inner(&self) -> RwLockReadGuard<'_, sourcemap::SourceMap> {
    self.0.read()
  }

  pub fn inner_mut(&mut self) -> RwLockWriteGuard<'_, sourcemap::SourceMap> {
    self.0.write()
  }

  pub fn from_inner(inner: sourcemap::SourceMap) -> Self {
    Self(Arc::new(RwLock::new(inner)))
  }
}

impl TryFrom<&[u8]> for SourceMap {
  type Error = sourcemap::Error;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
    Ok(Self::from_inner(sourcemap::SourceMap::from_slice(value)?))
  }
}

impl fmt::Display for SourceMap {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut w = Vec::new();
    self.0.write().to_writer(&mut w).map_err(|_| fmt::Error)?;
    let s = String::from_utf8(w).map_err(|_| fmt::Error)?;
    write!(f, "{}", s)
  }
}
