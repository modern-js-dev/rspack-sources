#![feature(test)]
#![allow(soft_unstable)]

extern crate test;
use test::Bencher;

use rspack_sources::{
  CachedSource, ConcatSource, MapOptions, Source, SourceMap, SourceMapSource,
  SourceMapSourceOptions,
};

const HELLOWORLD_JS: &'static str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-minify/files/helloworld.js"
));
const HELLOWORLD_JS_MAP: &'static str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-minify/files/helloworld.js.map"
));
const HELLOWORLD_MIN_JS: &'static str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-minify/files/helloworld.min.js"
));
const HELLOWORLD_MIN_JS_MAP: &'static str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-minify/files/helloworld.min.js.map"
));
const BUNDLE_JS: &'static str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-rollup/files/bundle.js"
));
const BUNDLE_JS_MAP: &'static str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-rollup/files/bundle.js.map"
));

#[bench]
fn benchmark_concat_generate_string(b: &mut Bencher) {
  let sms_minify = SourceMapSource::new(SourceMapSourceOptions {
    value: HELLOWORLD_MIN_JS,
    name: "helloworld.min.js",
    source_map: SourceMap::from_json(HELLOWORLD_MIN_JS_MAP).unwrap(),
    original_source: Some(HELLOWORLD_JS.to_string()),
    inner_source_map: Some(SourceMap::from_json(HELLOWORLD_JS_MAP).unwrap()),
    remove_original_source: false,
  });
  let sms_rollup = SourceMapSource::new(SourceMapSourceOptions {
    value: BUNDLE_JS,
    name: "bundle.js",
    source_map: SourceMap::from_json(BUNDLE_JS_MAP).unwrap(),
    original_source: None,
    inner_source_map: None,
    remove_original_source: false,
  });
  let concat = ConcatSource::new([sms_minify.clone(), sms_rollup.clone()]);

  b.iter(|| {
    concat
      .map(&MapOptions::default())
      .unwrap()
      .to_json()
      .unwrap();
  })
}

#[bench]
fn benchmark_concat_generate_string_with_cache(b: &mut Bencher) {
  let sms_minify = SourceMapSource::new(SourceMapSourceOptions {
    value: HELLOWORLD_MIN_JS,
    name: "helloworld.min.js",
    source_map: SourceMap::from_json(HELLOWORLD_MIN_JS_MAP).unwrap(),
    original_source: Some(HELLOWORLD_JS.to_string()),
    inner_source_map: Some(SourceMap::from_json(HELLOWORLD_JS_MAP).unwrap()),
    remove_original_source: false,
  });
  let sms_rollup = SourceMapSource::new(SourceMapSourceOptions {
    value: BUNDLE_JS,
    name: "bundle.js",
    source_map: SourceMap::from_json(BUNDLE_JS_MAP).unwrap(),
    original_source: None,
    inner_source_map: None,
    remove_original_source: false,
  });
  let concat = ConcatSource::new([sms_minify, sms_rollup]);
  let cached = CachedSource::new(concat);

  b.iter(|| {
    cached
      .map(&MapOptions::default())
      .unwrap()
      .to_json()
      .unwrap();
  })
}

#[bench]
fn benchmark_concat_generate_base64(b: &mut Bencher) {
  let sms_minify = SourceMapSource::new(SourceMapSourceOptions {
    value: HELLOWORLD_MIN_JS,
    name: "helloworld.min.js",
    source_map: SourceMap::from_json(HELLOWORLD_MIN_JS_MAP).unwrap(),
    original_source: Some(HELLOWORLD_JS.to_string()),
    inner_source_map: Some(SourceMap::from_json(HELLOWORLD_JS_MAP).unwrap()),
    remove_original_source: false,
  });
  let sms_rollup = SourceMapSource::new(SourceMapSourceOptions {
    value: BUNDLE_JS,
    name: "bundle.js",
    source_map: SourceMap::from_json(BUNDLE_JS_MAP).unwrap(),
    original_source: None,
    inner_source_map: None,
    remove_original_source: false,
  });
  let concat = ConcatSource::new([sms_minify.clone(), sms_rollup.clone()]);

  b.iter(|| {
    let json = concat
      .map(&MapOptions::default())
      .unwrap()
      .to_json()
      .unwrap();
    base64_simd::Base64::STANDARD.encode_to_boxed_str(json.as_bytes());
  })
}

#[bench]
fn benchmark_concat_generate_base64_with_cache(b: &mut Bencher) {
  let sms_minify = SourceMapSource::new(SourceMapSourceOptions {
    value: HELLOWORLD_MIN_JS,
    name: "helloworld.min.js",
    source_map: SourceMap::from_json(HELLOWORLD_MIN_JS_MAP).unwrap(),
    original_source: Some(HELLOWORLD_JS.to_string()),
    inner_source_map: Some(SourceMap::from_json(HELLOWORLD_JS_MAP).unwrap()),
    remove_original_source: false,
  });
  let sms_rollup = SourceMapSource::new(SourceMapSourceOptions {
    value: BUNDLE_JS,
    name: "bundle.js",
    source_map: SourceMap::from_json(BUNDLE_JS_MAP).unwrap(),
    original_source: None,
    inner_source_map: None,
    remove_original_source: false,
  });
  let concat = ConcatSource::new([sms_minify.clone(), sms_rollup.clone()]);
  let cached = CachedSource::new(concat);

  b.iter(|| {
    let json = cached
      .map(&MapOptions::default())
      .unwrap()
      .to_json()
      .unwrap();
    base64_simd::Base64::STANDARD.encode_to_boxed_str(json.as_bytes());
  })
}
