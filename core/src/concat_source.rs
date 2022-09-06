use sourcemap::SourceMapBuilder;

use crate::{BoxSource, MapOptions, Source, SourceMap};

pub struct ConcatSource {
  inner: Vec<u8>,
  children: Vec<BoxSource>,
}

impl ConcatSource {
  pub fn new<S: Into<BoxSource>>(items: impl IntoIterator<Item = S>) -> Self {
    let children: Vec<_> = items.into_iter().map(|s| s.into()).collect();
    let inner = children.iter().fold(Vec::new(), |mut acc, cur| {
      acc.extend(cur.buffer());
      acc
    });
    Self { inner, children }
  }

  pub fn add<S: Source + 'static>(&mut self, item: S) {
    let item: BoxSource = item.into();
    self.inner.extend(item.buffer());
    self.children.push(item);
  }

  // #[tracing::instrument(skip_all)]
  // pub fn generate_string(
  //   &mut self,
  //   gen_map_options: &MapOptions,
  // ) -> Result<Option<String>, Error> {
  //   let source_map = self.map(gen_map_options);
  //   let is_source_map_exist = source_map.is_some();

  //   let mut writer: Vec<u8> = Default::default();
  //   source_map.map(|sm| sm.to_writer(&mut writer));

  //   Ok(if is_source_map_exist {
  //     Some(String::from_utf8(writer)?)
  //   } else {
  //     None
  //   })
  // }

  // #[tracing::instrument(skip_all)]
  // pub fn generate_base64(
  //   &mut self,
  //   gen_map_options: &GenMapOption,
  // ) -> Result<Option<String>, Error> {
  //   let map_string = self.generate_string(gen_map_options)?;
  //   Ok(map_string.map(|s| {
  //     base64_simd::Base64::STANDARD
  //       .encode_to_boxed_str(s.as_bytes())
  //       .to_string()
  //   }))
  // }

  // #[tracing::instrument(skip_all)]
  // pub fn generate_url(&mut self, gen_map_options: &GenMapOption) -> Result<Option<String>, Error> {
  //   let map_base64 = self.generate_base64(gen_map_options)?;

  //   Ok(map_base64.map(|s| {
  //     // 43 is length of common prefix of javascript base64 blob
  //     let mut ret = String::with_capacity(s.len() + 43);
  //     ret += "data:application/json;charset=utf-8;base64,";
  //     ret += &s;
  //     ret
  //   }))
  // }
}

impl Source for ConcatSource {
  fn buffer(&self) -> &[u8] {
    &self.inner
  }

  #[tracing::instrument(skip_all)]
  fn map(&self, options: &MapOptions) -> Option<SourceMap> {
    let mut source_map_builder = SourceMapBuilder::new(options.file.as_deref());
    let mut cur_gen_line = 0u32;

    self.children.iter().for_each(|concattable| {
      // why not `lines`? `lines` will trim the trailing `\n`, which generates the wrong sourcemap
      let line_len = concattable.source().split('\n').count() - 1;
      concat_source_impl(
        &mut source_map_builder,
        cur_gen_line,
        concattable.as_ref(),
        options,
      );

      cur_gen_line += line_len as u32;
    });

    Some(SourceMap::from_inner(source_map_builder.into_sourcemap()))
  }
}

fn concat_source_impl(
  sm_builder: &mut SourceMapBuilder,
  mut cur_gen_line: u32,
  concattable: &dyn Source,
  gen_map_option: &MapOptions,
) {
  let mut source_map = concattable.map(gen_map_option);

  let mut prev_line = 0u32;

  if let Some(source_map) = &mut source_map {
    let source_map = source_map.inner_mut();
    source_map.tokens().for_each(|token| {
      let line_diff = token.get_dst_line() - prev_line;

      let raw_token = sm_builder.add(
        cur_gen_line + line_diff,
        token.get_dst_col(),
        token.get_src_line(),
        token.get_src_col(),
        token.get_source(),
        token.get_name(),
      );

      if gen_map_option.include_source_contents {
        sm_builder.set_source_contents(
          raw_token.src_id,
          source_map.get_source_contents(token.get_src_id()),
        );
      }

      cur_gen_line += line_diff;

      prev_line = token.get_dst_line();
    });
  }
}

#[cfg(test)]
mod tests {
  use std::convert::TryFrom;

  use crate::{OriginalSource, RawSource};

  use super::*;

  #[test]
  fn should_concat_two_sources() {
    let mut source = ConcatSource::new([
      Box::new(RawSource::from("Hello World\n".to_string())) as BoxSource,
      Box::new(OriginalSource::new(
        "console.log('test');\nconsole.log('test2');\n",
        "console.js",
      )),
    ]);
    source.add(OriginalSource::new("Hello2\n", "hello.md"));

    let expected_source = "Hello World\nconsole.log('test');\nconsole.log('test2');\nHello2\n";
    assert_eq!(source.size(), 62);
    assert_eq!(source.source(), expected_source);

    let expected_map1 = SourceMap::try_from(
      r#"{
        "version": 3,
        "mappings": ";AAAA;AACA;AACA,ACFA;AACA",
        "names": [],
        "sources": ["console.js", "hello.md"],
        "sourcesContent": [
          "console.log('test');\nconsole.log('test2');\n",
          "Hello2\n"
        ]
      }"#
        .as_bytes(),
    )
    .unwrap()
    .to_string();
    println!(
      "{}",
      source
        .map(&MapOptions {
          columns: false,
          ..Default::default()
        })
        .unwrap()
        .to_string(),
    );
    println!(
      "{}",
      source.map(&MapOptions::default()).unwrap().to_string(),
    );
    assert_eq!(
      source
        .map(&MapOptions {
          columns: false,
          ..Default::default()
        })
        .unwrap()
        .to_string(),
      expected_map1
    );

    let expected_map2 = SourceMap::try_from(
      r#"{
        "version": 3,
        "mappings": ";AAAA,oBAAoB;AACpB,qBAAqB;ACDrB",
        "names": [],
        "sources": ["console.js", "hello.md"],
        "sourcesContent": [
          "console.log('test');\nconsole.log('test2');\n",
          "Hello2\n"
        ]
      }"#
        .as_bytes(),
    )
    .unwrap()
    .to_string();
    assert_eq!(
      source.map(&MapOptions::default()).unwrap().to_string(),
      expected_map2
    );
  }

  #[test]
  fn should_be_able_to_handle_strings_for_all_methods() {
    let mut source = ConcatSource::new([
      Box::new(RawSource::from("Hello World\n".to_string())) as BoxSource,
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
    let expected_map1 = SourceMap::try_from(
      r#"{
        "version": 3,
        "mappings": ";AAAA;AACA;AACA",
        "names": [],
        "sources": ["console.js"],
        "sourcesContent": ["console.log('test');\nconsole.log('test2');\n"]
      }"#
        .as_bytes(),
    )
    .unwrap()
    .to_string();
    assert_eq!(source.size(), 76);
    assert_eq!(source.source(), expected_source);
    assert_eq!(source.buffer(), expected_source.as_bytes());
    assert_eq!(
      source
        .map(&MapOptions {
          columns: false,
          ..Default::default()
        })
        .unwrap()
        .to_string(),
      expected_map1
    );

    // TODO: test hash
  }

  #[test]
  fn should_return_null_as_map_when_only_generated_code_is_concatenated() {
    let source = ConcatSource::new([
      Box::new("Hello World\n") as BoxSource,
      Box::new(RawSource::from("Hello World\n".to_string())),
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
    assert_eq!(
      result_map.to_string(),
      "{\"version\":3,\"sources\":[],\"names\":[],\"mappings\":\"\"}"
    );
    assert_eq!(
      result_list_map.to_string(),
      "{\"version\":3,\"sources\":[],\"names\":[],\"mappings\":\"\"}"
    );
  }

  // FIX: can't concatenate in a single line
  #[test]
  fn should_allow_to_concatenate_in_a_single_line() {
    let source = ConcatSource::new([
      Box::new(OriginalSource::new("Hello", "hello1.txt")) as BoxSource,
      Box::new(" "),
      Box::new(OriginalSource::new("World ", "world1.txt")),
      Box::new("is here\n"),
      Box::new(OriginalSource::new("Hello\n", "hello2.txt")),
      Box::new(" \n"),
      Box::new(OriginalSource::new("World\n", "world2.txt")),
      Box::new("is here"),
    ]);

    println!(
      "{}",
      &source.map(&MapOptions::default()).unwrap().to_string()
    );

    assert_eq!(
      &source
        .map(&MapOptions::default())
        .unwrap()
        .to_string(),
        "{\"version\":3,\"sources\":[\"hello1.txt\",\"world1.txt\",\"hello2.txt\",\"world2.txt\"],\"sourcesContent\":[\"Hello\",\"World \",\"Hello\\n\",\"World\\n\"],\"names\":[],\"mappings\":\"AAAA;;ACAA;;;ACAA;;;;ACAA\"}",
    );
    assert_eq!(
      &source.source(),
      "Hello World is here\nHello\n \nWorld\nis here",
    );
  }

  #[test]
  fn should_allow_to_concat_buffer_sources() {
    let source = ConcatSource::new([
      Box::new("a") as BoxSource,
      Box::new(RawSource::from(Vec::from("b"))),
      Box::new("c"),
    ]);
    assert_eq!(source.source(), "abc");
    assert_eq!(
      source.map(&MapOptions::default()).unwrap().to_string(),
      "{\"version\":3,\"sources\":[],\"names\":[],\"mappings\":\"\"}",
    );
  }
}
