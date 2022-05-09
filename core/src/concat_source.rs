use magic_string::MagicString;
use sourcemap::{SourceMap, SourceMapBuilder};

use crate::{
  helpers::get_map,
  source::{GenMapOption, Source},
  source_map_source::SourceMapSource,
};

pub enum ConcattableSource {
  SourceMapSource(SourceMapSource),
  // TODO:
  // ConcatSource(ConcatSource),
  // CachedSource
}

pub struct ConcatSource {
  children: Vec<ConcattableSource>,
}

impl ConcatSource {
  pub fn new(items: Vec<ConcattableSource>) -> Self {
    Self { children: items }
  }

  pub fn add(&mut self, item: ConcattableSource) {
    self.children.push(item);
  }

  pub(crate) fn concat_each_impl(
    sm_builder: &mut SourceMapBuilder,
    mut cur_gen_line: u32,
    concattable: &mut ConcattableSource,
  ) {
    match concattable {
      ConcattableSource::SourceMapSource(s) => {
        s.ensure_original_source();

        let remapped_source_map = s.remap_with_inner_sourcemap();
        let source_map = remapped_source_map
          .as_ref()
          .unwrap_or_else(|| &s.source_map);

        let mut prev_line = 0u32;

        source_map.tokens().for_each(|token| {
          println!("token {:#?}", token.get_raw_token());
          let line_diff = token.get_dst_line() - prev_line;

          let raw_token = sm_builder.add(
            cur_gen_line + line_diff,
            token.get_dst_col(),
            token.get_src_line(),
            token.get_src_col(),
            token.get_source(),
            token.get_name(),
          );

          sm_builder.set_source_contents(
            raw_token.src_id,
            source_map.get_source_contents(token.get_src_id()),
          );

          cur_gen_line += line_diff;

          prev_line = token.get_dst_line();
        });
      }
    }
  }
}

impl Source for ConcatSource {
  fn source(&self) -> String {
    let mut code = "".to_owned();
    self.children.iter().for_each(|child| {
      let mut source = match child {
        ConcattableSource::SourceMapSource(s) => s.source(),
        // ConcattableSource::ConcatSource(s) => s.source(),
      };
      source += "\n";
      code += &source;
    });
    code
  }

  fn map(&mut self, option: GenMapOption) -> Option<SourceMap> {
    let mut source_map_builder = SourceMapBuilder::new(None);
    let mut cur_gen_line = 0u32;

    self.children.iter_mut().for_each(|concattable| {
      let line_len = match concattable {
        ConcattableSource::SourceMapSource(s) => s.source().lines().count(),
      };

      ConcatSource::concat_each_impl(&mut source_map_builder, cur_gen_line, concattable);

      cur_gen_line += line_len as u32 + 1;
    });

    // TODO:
    // get_map(option)

    Some(source_map_builder.into_sourcemap())
  }
}

#[test]
fn test_concat_source() {
  let base_fixure = ::std::path::PathBuf::from("tests/fixtures/transpile-minify/files/helloworld");

  let mut original_map_path = base_fixure.clone();
  original_map_path.set_extension("js.map");
  let mut transformed_map_path = base_fixure.clone();
  transformed_map_path.set_extension("min.js.map");

  let mut original_code_path = base_fixure.clone();
  original_code_path.set_extension("js");
  let mut transformed_code_path = base_fixure.clone();
  transformed_code_path.set_extension("min.js");

  let original_map_buf = ::std::fs::read(original_map_path).expect("unable to find test fixture");
  let transformed_map_buf =
    ::std::fs::read(transformed_map_path).expect("unable to find test fixture");
  let original_code_buf = ::std::fs::read(original_code_path).expect("unable to find test fixture");
  let transformed_code_buf =
    ::std::fs::read(transformed_code_path).expect("unable to find test fixture");

  let mut source_map_source = SourceMapSource::from_slice(crate::SourceMapSourceSliceOptions {
    source_code: &transformed_code_buf,
    name: "helloworld.min.js".into(),
    source_map: sourcemap::SourceMap::from_slice(&transformed_map_buf).unwrap(),
    original_source: Some(&original_code_buf),
    inner_source_map: Some(sourcemap::SourceMap::from_slice(&original_map_buf).unwrap()),
    remove_original_source: false,
  })
  .expect("failed");

  let map_buf =
    ::std::fs::read("tests/fixtures/transpile-rollup/files/bundle.js.map").expect("failed");

  let js_buf = ::std::fs::read("tests/fixtures/transpile-rollup/files/bundle.js").expect("failed");

  let mut source_map_source_rollup =
    SourceMapSource::from_slice(crate::SourceMapSourceSliceOptions {
      source_code: &js_buf,
      name: "bundle.js".into(),
      source_map: sourcemap::SourceMap::from_slice(&map_buf).unwrap(),
      original_source: None,
      inner_source_map: None,
      remove_original_source: false,
    })
    .expect("failed");

  let mut concat_source = ConcatSource::new(vec![
    ConcattableSource::SourceMapSource(source_map_source_rollup),
    ConcattableSource::SourceMapSource(source_map_source),
  ]);

  let mut sm_writer: Vec<u8> = Default::default();
  concat_source
    .map(GenMapOption { columns: true })
    .expect("failed")
    .to_writer(&mut sm_writer);

  println!("generated code {}", concat_source.source());
  println!(
    "generated sm {}",
    String::from_utf8(sm_writer).expect("failed")
  );
}
