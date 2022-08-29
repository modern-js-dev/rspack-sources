use std::collections::HashMap;

use smol_str::SmolStr;
use sourcemap::SourceMap;

use crate::{utils::Lrc, MapOptions, Source};

pub struct CachedSource<T: Source> {
  inner: T,
  cached_map: HashMap<MapOptions, Option<Lrc<SourceMap>>>,
  cached_code: Option<SmolStr>,
}

impl<T: Source> CachedSource<T> {
  pub fn new(source: T) -> Self {
    Self {
      inner: source,
      cached_code: Default::default(),
      cached_map: Default::default(),
    }
  }

  pub fn into_inner(self) -> T {
    self.inner
  }
}

impl<T: Source> Source for CachedSource<T> {
  fn map(&mut self, gen_map_option: &MapOptions) -> Option<Lrc<SourceMap>> {
    if let Some(source_map) = self.cached_map.get(gen_map_option) {
      source_map.as_ref().cloned()
    } else {
      let map = self.inner.map(gen_map_option);
      self.cached_map.insert(gen_map_option.clone(), map.clone());
      map
    }
  }

  fn source(&mut self) -> SmolStr {
    if let Some(cached_code) = &self.cached_code {
      return cached_code.clone();
    }
    let code = self.inner.source();
    self.cached_code = Some(code.clone());
    code
  }
}
