use std::{borrow::Cow, collections::HashMap};

use parking_lot::Mutex;

use crate::{MapOptions, Source, SourceMap};

pub struct CachedSource<T> {
  inner: T,
  cached_maps: Mutex<HashMap<MapOptions, Option<SourceMap>>>,
}

impl<T> CachedSource<T> {
  pub fn new(inner: T) -> Self {
    Self {
      inner,
      cached_maps: Mutex::new(HashMap::new()),
    }
  }

  pub fn into_inner(self) -> T {
    self.inner
  }
}

impl<T: Source> Source for CachedSource<T> {
  fn map(&self, options: &MapOptions) -> Option<SourceMap> {
    let mut cached_maps = self.cached_maps.lock();
    if let Some(cache) = cached_maps.get(options) {
      cache.to_owned()
    } else {
      let map = self.inner.map(options);
      cached_maps.insert(options.to_owned(), map.clone());
      map
    }
  }

  fn buffer(&self) -> &[u8] {
    self.inner.buffer()
  }

  fn source(&self) -> Cow<str> {
    self.inner.source()
  }

  fn size(&self) -> usize {
    self.inner.size()
  }
}
