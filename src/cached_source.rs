use std::{
  borrow::Cow,
  hash::{BuildHasherDefault, Hash, Hasher},
  sync::{Arc, OnceLock},
};

use dashmap::{mapref::entry::Entry, DashMap};
use rustc_hash::FxHasher;

use crate::{
  helpers::{
    stream_and_get_source_and_map, stream_chunks_of_raw_source,
    stream_chunks_of_source_map, StreamChunks,
  },
  rope::Rope,
  MapOptions, Source, SourceMap,
};

/// It tries to reused cached results from other methods to avoid calculations,
/// usually used after modify is finished.
///
/// - [webpack-sources docs](https://github.com/webpack/webpack-sources/#cachedsource).
///
/// ```
/// use rspack_sources::{
///   BoxSource, CachedSource, ConcatSource, MapOptions, OriginalSource,
///   RawSource, Source, SourceExt, SourceMap,
/// };
///
/// let mut concat = ConcatSource::new([
///   RawSource::from("Hello World\n".to_string()).boxed(),
///   OriginalSource::new(
///     "console.log('test');\nconsole.log('test2');\n",
///     "console.js",
///   )
///   .boxed(),
/// ]);
/// concat.add(OriginalSource::new("Hello2\n", "hello.md"));
///
/// let cached = CachedSource::new(concat);
///
/// assert_eq!(
///   cached.source(),
///   "Hello World\nconsole.log('test');\nconsole.log('test2');\nHello2\n"
/// );
/// // second time will be fast.
/// assert_eq!(
///   cached.source(),
///   "Hello World\nconsole.log('test');\nconsole.log('test2');\nHello2\n"
/// );
/// ```

pub struct CachedSource<T: 'static> {
  inner: CachedSourceInner<T>,
}

#[ouroboros::self_referencing]
pub struct CachedSourceInner<T: 'static> {
  inner: Arc<T>,
  #[not_covariant]
  #[borrows(inner)]
  cached_rope: Arc<OnceLock<Rope<'this>>>,
  cached_hash: Arc<OnceLock<u64>>,
  cached_maps:
    Arc<DashMap<MapOptions, Option<SourceMap>, BuildHasherDefault<FxHasher>>>,
}

impl<T: Source> CachedSource<T> {
  fn get_rope(&self) -> &Rope<'_> {
    self
      .inner
      .with(|cache| cache.cached_rope.get_or_init(|| cache.inner.rope()))
  }
}

impl<T> CachedSource<T> {
  /// Create a [CachedSource] with the original [Source].
  pub fn new(inner: T) -> Self {
    Self {
      inner: CachedSourceInner::new(
        Arc::new(inner),
        |_| Default::default(),
        Default::default(),
        Default::default(),
      ),
    }
  }

  /// Get the original [Source].
  pub fn original(&self) -> &T {
    self.inner.borrow_inner()
  }
}

impl<T: Source + Hash + PartialEq + Eq + 'static> Source for CachedSource<T> {
  fn source(&self) -> Cow<str> {
    Cow::Owned(self.get_rope().to_string())
  }

  fn rope(&self) -> Rope<'_> {
    self.get_rope().clone()
  }

  fn buffer(&self) -> Cow<[u8]> {
    self.inner.borrow_inner().buffer()
  }

  fn size(&self) -> usize {
    self.source().len()
  }

  fn map(&self, options: &MapOptions) -> Option<SourceMap> {
    if let Some(map) = self.inner.borrow_cached_maps().get(options) {
      map.clone()
    } else {
      let map = self.inner.borrow_inner().map(options);
      self
        .inner
        .borrow_cached_maps()
        .insert(options.clone(), map.clone());
      map
    }
  }

  fn to_writer(&self, writer: &mut dyn std::io::Write) -> std::io::Result<()> {
    self.inner.borrow_inner().to_writer(writer)
  }
}

impl<T: Source + Hash + PartialEq + Eq + 'static> StreamChunks
  for CachedSource<T>
{
  fn stream_chunks<'a>(
    &'a self,
    options: &MapOptions,
    on_chunk: crate::helpers::OnChunk<'_, 'a>,
    on_source: crate::helpers::OnSource<'_, 'a>,
    on_name: crate::helpers::OnName<'_, 'a>,
  ) -> crate::helpers::GeneratedInfo {
    let cached_map = self.inner.borrow_cached_maps().entry(options.clone());
    match cached_map {
      Entry::Occupied(entry) => {
        let source = self.rope();
        if let Some(map) = entry.get() {
          #[allow(unsafe_code)]
          // SAFETY: We guarantee that once a `SourceMap` is stored in the cache, it will never be removed.
          // Therefore, even if we force its lifetime to be longer, the reference remains valid.
          // This is based on the following assumptions:
          // 1. `SourceMap` will be valid for the entire duration of the application.
          // 2. The cached `SourceMap` will not be manually removed or replaced, ensuring the reference's safety.
          let map =
            unsafe { std::mem::transmute::<&SourceMap, &'a SourceMap>(map) };
          stream_chunks_of_source_map(
            source, map, on_chunk, on_source, on_name, options,
          )
        } else {
          stream_chunks_of_raw_source(
            source, options, on_chunk, on_source, on_name,
          )
        }
      }
      Entry::Vacant(entry) => {
        let (generated_info, map) = stream_and_get_source_and_map(
          self.inner.borrow_inner() as &T,
          options,
          on_chunk,
          on_source,
          on_name,
        );
        entry.insert(map);
        generated_info
      }
    }
  }
}

impl<T> Clone for CachedSource<T> {
  fn clone(&self) -> Self {
    // Self {
    //   inner: self.inner.clone(),
    //   cached_rope: Default::default(),
    //   cached_hash: self.cached_hash.clone(),
    //   cached_maps: self.cached_maps.clone(),
    // }
    todo!()
  }
}

impl<T: Source + Hash + PartialEq + Eq + 'static> Hash for CachedSource<T> {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    (self.inner.borrow_cached_hash().get_or_init(|| {
      let mut hasher = FxHasher::default();
      self.original().hash(&mut hasher);
      hasher.finish()
    }))
    .hash(state);
  }
}

impl<T: PartialEq> PartialEq for CachedSource<T> {
  fn eq(&self, other: &Self) -> bool {
    self.inner.borrow_inner() == other.inner.borrow_inner()
  }
}

impl<T: Eq> Eq for CachedSource<T> {}

impl<T: std::fmt::Debug> std::fmt::Debug for CachedSource<T> {
  fn fmt(
    &self,
    f: &mut std::fmt::Formatter<'_>,
  ) -> Result<(), std::fmt::Error> {
    f.debug_struct("CachedSource")
      .field("inner", self.inner.borrow_inner().as_ref())
      .field("cached_hash", self.inner.borrow_cached_hash().as_ref())
      .field(
        "cached_maps",
        &(!self.inner.borrow_cached_maps().is_empty()),
      )
      .finish()
  }
}

#[cfg(test)]
mod tests {
  use crate::{
    ConcatSource, OriginalSource, RawSource, ReplaceSource, SourceExt,
    SourceMapSource, WithoutOriginalOptions,
  };

  use super::*;

  #[test]
  fn line_number_should_not_add_one() {
    let source = ConcatSource::new([
      CachedSource::new(RawSource::from("\n")).boxed(),
      SourceMapSource::new(WithoutOriginalOptions {
        value: "\nconsole.log(1);\n".to_string(),
        name: "index.js".to_string(),
        source_map: SourceMap::new(
          ";AACA",
          vec!["index.js".into()],
          vec!["// DELETE IT\nconsole.log(1)".into()],
          vec![],
        ),
      })
      .boxed(),
    ]);
    let map = source.map(&Default::default()).unwrap();
    assert_eq!(map.mappings(), ";;AACA");
  }

  #[test]
  fn should_allow_to_store_and_share_cached_data() {
    let original = OriginalSource::new("Hello World", "test.txt");
    let source = CachedSource::new(original);
    let clone = source.clone();

    // fill up cache
    let map_options = MapOptions::default();
    source.source();
    source.buffer();
    source.size();
    source.map(&map_options);

    assert_eq!(
      *clone
        .inner
        .borrow_cached_maps()
        .get(&map_options)
        .unwrap()
        .value(),
      source.map(&map_options)
    );
  }

  #[test]
  fn should_return_the_correct_size_for_binary_files() {
    let source = OriginalSource::new(
      String::from_utf8(vec![0; 256]).unwrap(),
      "file.wasm",
    );
    let cached_source = CachedSource::new(source);

    assert_eq!(cached_source.size(), 256);
    assert_eq!(cached_source.size(), 256);
  }

  #[test]
  fn should_return_the_correct_size_for_cached_binary_files() {
    let source = OriginalSource::new(
      String::from_utf8(vec![0; 256]).unwrap(),
      "file.wasm",
    );
    let cached_source = CachedSource::new(source);

    cached_source.source();
    assert_eq!(cached_source.size(), 256);
    assert_eq!(cached_source.size(), 256);
  }

  #[test]
  fn should_return_the_correct_size_for_text_files() {
    let source = OriginalSource::new("TestTestTest", "file.js");
    let cached_source = CachedSource::new(source);

    assert_eq!(cached_source.size(), 12);
    assert_eq!(cached_source.size(), 12);
  }

  #[test]
  fn should_return_the_correct_size_for_cached_text_files() {
    let source = OriginalSource::new("TestTestTest", "file.js");
    let cached_source = CachedSource::new(source);

    cached_source.source();
    assert_eq!(cached_source.size(), 12);
    assert_eq!(cached_source.size(), 12);
  }

  #[test]
  fn should_produce_correct_output_for_cached_raw_source() {
    let map_options = MapOptions {
      columns: true,
      final_source: true,
    };

    let source = RawSource::from("Test\nTest\nTest\n");
    let mut on_chunk_count = 0;
    let mut on_source_count = 0;
    let mut on_name_count = 0;
    let generated_info = source.stream_chunks(
      &map_options,
      &mut |_chunk, _mapping| {
        on_chunk_count += 1;
      },
      &mut |_source_index, _source, _source_content| {
        on_source_count += 1;
      },
      &mut |_name_index, _name| {
        on_name_count += 1;
      },
    );

    let cached_source = CachedSource::new(source);
    cached_source.stream_chunks(
      &map_options,
      &mut |_chunk, _mapping| {},
      &mut |_source_index, _source, _source_content| {},
      &mut |_name_index, _name| {},
    );

    let mut cached_on_chunk_count = 0;
    let mut cached_on_source_count = 0;
    let mut cached_on_name_count = 0;
    let cached_generated_info = cached_source.stream_chunks(
      &map_options,
      &mut |_chunk, _mapping| {
        cached_on_chunk_count += 1;
      },
      &mut |_source_index, _source, _source_content| {
        cached_on_source_count += 1;
      },
      &mut |_name_index, _name| {
        cached_on_name_count += 1;
      },
    );

    assert_eq!(on_chunk_count, cached_on_chunk_count);
    assert_eq!(on_source_count, cached_on_source_count);
    assert_eq!(on_name_count, cached_on_name_count);
    assert_eq!(generated_info, cached_generated_info);
  }

  #[test]
  fn should_have_correct_buffer_if_cache_buffer_from_cache_source() {
    let buf = vec![128u8];
    let source = CachedSource::new(RawSource::from(buf.clone()));

    source.source();
    assert_eq!(source.buffer(), buf.as_slice());
  }

  #[test]
  fn hash_should_different_when_map_are_different() {
    let hash1 = {
      let mut source =
        ReplaceSource::new(OriginalSource::new("Hello", "hello.txt").boxed());
      source.insert(5, " world", None);
      let cache = CachedSource::new(source);
      let mut hasher = FxHasher::default();
      cache.hash(&mut hasher);
      hasher.finish()
    };

    let hash2 = {
      let source = OriginalSource::new("Hello world", "hello.txt").boxed();
      let cache = CachedSource::new(source);
      let mut hasher = FxHasher::default();
      cache.hash(&mut hasher);
      hasher.finish()
    };

    assert!(hash1 != hash2);
  }
}
