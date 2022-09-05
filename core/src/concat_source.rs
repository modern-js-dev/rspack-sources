use parcel_sourcemap::SourceMap;

use crate::{
  source::{BoxSource, MapOptions, Source},
  Error,
};

pub struct ConcatSource {
  children: Vec<BoxSource>,
}

impl ConcatSource {
  pub fn new<S: Into<BoxSource>>(items: impl IntoIterator<Item = S>) -> Self {
    Self {
      children: items
        .into_iter()
        .map(|s| s.into())
        .collect::<Vec<BoxSource>>(),
    }
  }

  pub fn add<S: Source + 'static>(&mut self, item: S) {
    self.children.push(item.into());
  }

  #[tracing::instrument(skip_all)]
  pub fn generate_string(&mut self, gen_map_options: &MapOptions) -> Result<Option<String>, Error> {
    let source_map = self.map(gen_map_options);

    Ok(
      source_map
        .map(|mut sm| sm.to_json(None))
        .transpose()
        .unwrap(),
    ) // TODO: error
  }

  #[tracing::instrument(skip_all)]
  pub fn generate_base64(&mut self, gen_map_options: &MapOptions) -> Result<Option<String>, Error> {
    let map_string = self.generate_string(gen_map_options)?;
    Ok(map_string.map(|s| {
      base64_simd::Base64::STANDARD
        .encode_to_boxed_str(s.as_bytes())
        .to_string()
    }))
  }

  #[tracing::instrument(skip_all)]
  pub fn generate_url(&mut self, gen_map_options: &MapOptions) -> Result<Option<String>, Error> {
    let map_base64 = self.generate_base64(gen_map_options)?;

    Ok(map_base64.map(|s| {
      // 43 is length of common prefix of javascript base64 blob
      let mut ret = String::with_capacity(s.len() + 43);
      ret += "data:application/json;charset=utf-8;base64,";
      ret += &s;
      ret
    }))
  }
}

impl Source for ConcatSource {
  #[tracing::instrument(skip_all)]
  fn source(&mut self) -> String {
    self
      .children
      .iter_mut()
      .map(|child| child.source())
      .collect::<Vec<_>>()
      .join("")
  }

  #[tracing::instrument(skip_all)]
  fn map(&mut self, option: &MapOptions) -> Option<SourceMap> {
    let mut source_map_builder = SourceMap::new("/");
    let mut line_offset = 0;

    self.children.iter_mut().for_each(|concattable| {
      let lines = concattable.source().split("\n").count() - 1;
      if let Some(ref mut concattable_source_map) = concattable.map(option) {
        source_map_builder.add_sourcemap(concattable_source_map, line_offset as i64);
      }

      line_offset += lines;
    });

    Some(source_map_builder)
  }

  fn size(&self) -> usize {
    self.children.iter().fold(0, |mut size, child| {
      size += child.size();
      size
    })
  }
}

#[cfg(test)]
mod tests {
  use crate::{OriginalSource, RawSource};

  use super::*;

  #[test]
  fn should_concat_two_sources() {
    let mut source = ConcatSource::new([
      Box::new(RawSource::new("Hello World\n")) as BoxSource,
      Box::new(OriginalSource::new(
        "console.log('test');\nconsole.log('test2');\n",
        "console.js",
      )),
    ]);
    source.add(OriginalSource::new("Hello2\n", "hello.md"));

    let expected_source = "Hello World\nconsole.log('test');\nconsole.log('test2');\nHello2\n";
    assert_eq!(source.size(), 62);
    assert_eq!(source.source(), expected_source);

    let expected_map1 = SourceMap::from_json(
      "/",
      r#"{
        "version": 3,
        "mappings": ";AAAA;AACA;ACDA;AACA",
        "names": [],
        "sources": ["console.js", "hello.md"],
        "sourcesContent": [
          "console.log('test');\nconsole.log('test2');\n",
          "Hello2\n"
        ]
      }"#,
    )
    .unwrap()
    .to_json(None)
    .unwrap();
    assert_eq!(
      source
        .map(&MapOptions {
          columns: false,
          ..Default::default()
        })
        .unwrap()
        .to_json(None)
        .unwrap(),
      expected_map1
    );

    let expected_map2 = SourceMap::from_json(
      "/",
      r#"{
        "version": 3,
        "mappings": ";AAAA,oBAAoB;AACpB,qBAAqB;ACDrB",
        "names": [],
        "sources": ["console.js", "hello.md"],
        "sourcesContent": [
          "console.log('test');\nconsole.log('test2');\n",
          "Hello2\n"
        ]
      }"#,
    )
    .unwrap()
    .to_json(None)
    .unwrap();
    assert_eq!(
      source
        .map(&MapOptions::default())
        .unwrap()
        .to_json(None)
        .unwrap(),
      expected_map2
    );
  }

  #[test]
  fn should_be_able_to_handle_strings_for_all_methods() {
    let mut source = ConcatSource::new([
      Box::new(RawSource::new("Hello World\n")) as BoxSource,
      Box::new(OriginalSource::new(
        "console.log('test');\nconsole.log('test2');\n",
        "console.js",
      )),
    ]);
    let inner_source = ConcatSource::new(["(", "'string'", ")"]);
    source.add("console");
    source.add(".");
    source.add("log");
    source.add(inner_source);
    let expected_source =
      "Hello World\nconsole.log('test');\nconsole.log('test2');\nconsole.log('string')";
    let expected_map1 = SourceMap::from_json(
      "/",
      r#"{
        "version": 3,
        "mappings": ";AAAA;AACA;AACA",
        "names": [],
        "sources": ["console.js"],
        "sourcesContent": ["console.log('test');\nconsole.log('test2');\n"]
      }"#,
    )
    .unwrap()
    .to_json(None)
    .unwrap();
    assert_eq!(source.size(), 76);
    assert_eq!(source.source(), expected_source);
    // TODO: assert_eq!(source.buffer(), expected_source);
    assert_eq!(
      source
        .map(&MapOptions {
          columns: false,
          ..Default::default()
        })
        .unwrap()
        .to_json(None)
        .unwrap(),
      expected_map1
    );

    // TODO: test hash
  }

  #[test]
  fn should_return_null_as_map_when_only_generated_code_is_concatenated() {
    let mut source = ConcatSource::new([
      Box::new("Hello World\n") as BoxSource,
      Box::new(RawSource::new("Hello World\n")),
      Box::new(""),
    ]);

    let result_text = source.source();
    let result_map = source.map(&MapOptions::default()).unwrap();
    let result_list_map = source
      .map(&MapOptions {
        columns: false,
        ..Default::default()
      })
      .unwrap();

    assert_eq!(result_text, "Hello World\nHello World\n");
    assert!(result_map.get_mappings().is_empty());
    assert!(result_list_map.get_mappings().is_empty());
  }

  // FIX: can't concatenate in a single line for now, see https://github.com/parcel-bundler/source-map/issues/114
  #[test]
  fn should_allow_to_concatenate_in_a_single_line() {
    let mut source = ConcatSource::new([
      Box::new(OriginalSource::new("Hello", "hello1.txt")) as BoxSource,
      Box::new(" "),
      Box::new(OriginalSource::new("World ", "world1.txt")),
      Box::new("is here\n"),
      Box::new(OriginalSource::new("Hello\n", "hello2.txt")),
      Box::new(" \n"),
      Box::new(OriginalSource::new("World\n", "world2.txt")),
      Box::new("is here"),
    ]);

    assert_eq!(
      &source
        .map(&MapOptions::default())
        .unwrap()
        .to_json(None)
        .unwrap(),
      "{\"version\":3,\"sourceRoot\":null,\"mappings\":\"ACAA;ACAA;;ACAA\",\"sources\":[\"hello1.txt\",\"world1.txt\",\"hello2.txt\",\"world2.txt\"],\"sourcesContent\":[\"Hello\",\"World \",\"Hello\\n\",\"World\\n\"],\"names\":[]}",
    );
    assert_eq!(
      &source.source(),
      "Hello World is here\nHello\n \nWorld\nis here",
    );
  }

  #[test]
  fn should_allow_to_concat_buffer_sources() {
    let mut source = ConcatSource::new([
      Box::new("a") as BoxSource,
      Box::new(RawSource::new("b")),
      Box::new("c"),
    ]);
    assert_eq!(source.source(), "abc");
    assert_eq!(
      source
        .map(&MapOptions::default())
        .unwrap()
        .to_json(None)
        .unwrap(),
        "{\"version\":3,\"sourceRoot\":null,\"mappings\":\"\",\"sources\":[],\"sourcesContent\":[],\"names\":[]}",
    );
  }
}

fn with_readable_mappings(sourcemap: &SourceMap) -> String {
  let mut first = true;
  let mut last_line = 0;
  sourcemap
    .get_mappings()
    .into_iter()
    .map(|token| {
      format!(
        "{}:{} ->{} {}:{}{}",
        if !first && token.generated_line == last_line {
          ", ".to_owned()
        } else {
          first = false;
          last_line = token.generated_line;
          format!("\n{}", token.generated_line + 1)
        },
        token.generated_column,
        token.original.map_or("".to_owned(), |original| format!(
          " [{}]",
          sourcemap.get_source(original.source).unwrap()
        )),
        token.original.unwrap().original_line + 1,
        token.original.unwrap().original_column,
        token
          .original
          .and_then(|original| original.name)
          .map_or("".to_owned(), |name| format!(
            " ({})",
            sourcemap.get_name(name).unwrap()
          )),
      )
    })
    .collect()
}
