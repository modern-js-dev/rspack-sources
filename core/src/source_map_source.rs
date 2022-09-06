use std::borrow::Cow;

use parking_lot::RwLockReadGuard;
use sourcemap::{SourceMapBuilder, Token};

use crate::{MapOptions, Source, SourceMap};

#[derive(Debug, Clone)]
pub struct SourceMapSource {
  source: String,
  name: String,
  source_map: SourceMap,
  original_source: Option<String>,
  inner_source_map: Option<SourceMap>,
  remove_original_source: bool,
}

pub struct SourceMapSourceOptions<S, F, M, O, I> {
  pub source: S,
  pub name: F,
  pub source_map: M,
  pub original_source: Option<O>,
  pub inner_source_map: Option<I>,
  pub remove_original_source: bool,
}

impl SourceMapSource {
  pub fn new<S, F, M, O, I>(options: SourceMapSourceOptions<S, F, M, O, I>) -> Self
  where
    S: Into<String>,
    F: Into<String>,
    M: Into<SourceMap>,
    O: Into<String>,
    I: Into<SourceMap>,
  {
    Self {
      source: options.source.into(),
      name: options.name.into(),
      source_map: options.source_map.into(),
      original_source: options.original_source.map(Into::into),
      inner_source_map: options.inner_source_map.map(Into::into),
      remove_original_source: options.remove_original_source,
    }
  }

  #[tracing::instrument(skip_all)]
  pub(crate) fn remap_with_inner_sourcemap(&self, options: &MapOptions) -> Option<SourceMap> {
    let mut source_map_builder = SourceMapBuilder::new(Some(&self.name));

    if let Some(inner_source_map) = &self.inner_source_map {
      let source_map = self.source_map.inner();
      let inner_source_map = inner_source_map.inner();
      source_map.tokens().for_each(|token| {
        let (original_token, source_content) =
          find_original_token(&source_map, &inner_source_map, &token);

        let raw_token = source_map_builder.add(
          token.get_dst_line(),
          token.get_dst_col(),
          original_token.get_src_line(),
          original_token.get_src_col(),
          original_token.get_source(),
          original_token.get_name(),
        );

        if options.include_source_contents && !self.remove_original_source {
          source_map_builder.set_source_contents(raw_token.src_id, source_content);
        }
      });

      return Some(SourceMap::from_inner(source_map_builder.into_sourcemap()));
    }

    None
  }
}

impl Source for SourceMapSource {
  fn buffer(&self) -> &[u8] {
    self.source.as_bytes()
  }

  fn source(&self) -> Cow<str> {
    Cow::Borrowed(&self.source)
  }

  fn size(&self) -> usize {
    self.source.len()
  }

  #[tracing::instrument(skip_all)]
  fn map(&self, option: &MapOptions) -> Option<SourceMap> {
    self
      .remap_with_inner_sourcemap(option)
      .or(Some(self.source_map.clone()))
  }
}

fn ensure_source_map(
  mut source_map: SourceMap,
  name: &str,
  original_source: Option<String>,
) -> SourceMap {
  let mut inner = source_map.inner_mut();
  let current_file_name = name;
  let source_idx = inner.sources().enumerate().find_map(|(idx, source)| {
    if source == current_file_name {
      Some(idx)
    } else {
      None
    }
  });

  if let Some(source_idx) = source_idx {
    if inner.get_source(source_idx as u32).is_none() {
      inner.set_source_contents(
        source_idx as u32,
        original_source.as_ref().map(|s| s.as_str()),
      );
    }
  }

  drop(inner);
  source_map
}

fn load_source_contents<'a>(
  source_map: &'a RwLockReadGuard<'a, sourcemap::SourceMap>,
  token: &Token,
) -> Option<&'a str> {
  source_map.get_source_contents(token.get_src_id())
}

fn find_original_token<'a>(
  source_map: &'a RwLockReadGuard<'a, sourcemap::SourceMap>,
  inner_source_map: &'a RwLockReadGuard<'a, sourcemap::SourceMap>,
  token: &'a Token<'a>,
) -> (Token<'a>, Option<&'a str>) {
  let source = token.get_source();
  let src_line = token.get_src_line();
  let src_col = token.get_src_col();

  if inner_source_map.get_file() == source {
    if let Some(original_token) = inner_source_map.lookup_token(src_line, src_col) {
      (
        original_token,
        inner_source_map.get_source_contents(original_token.get_src_id()),
      )
    } else {
      (*token, load_source_contents(source_map, token))
    }
  } else {
    (*token, load_source_contents(source_map, token))
  }
}

#[test]
fn test_source_map_source() {
  let transformed_map = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/transpile-minify/files/helloworld.min.js.map"
  ));
  let transformed_code = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/transpile-minify/files/helloworld.min.js"
  ));
  let original_map = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/transpile-minify/files/helloworld.js.map"
  ));
  let original_code = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/transpile-minify/files/helloworld.js"
  ));

  let source_map_source = SourceMapSource::new(SourceMapSourceOptions {
    source: transformed_code,
    name: "helloworld.min.js",
    source_map: SourceMap::try_from(transformed_map.as_bytes()).unwrap(),
    original_source: Some(original_code),
    inner_source_map: Some(SourceMap::try_from(original_map.as_bytes()).unwrap()),
    remove_original_source: false,
  });

  let new_source_map = source_map_source.map(&Default::default()).unwrap();
  let new_source_map = new_source_map.inner();
  let token = new_source_map.lookup_token(15, 47).unwrap();

  assert_eq!(token.get_source(), Some("helloworld.mjs"));
  assert_eq!(token.get_src_col(), 20);
  assert_eq!(token.get_src_line(), 18);
  assert_eq!(token.get_name(), Some("alert"));
}
