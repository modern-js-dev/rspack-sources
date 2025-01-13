#![allow(missing_docs)]

use std::collections::HashMap;

#[cfg(not(codspeed))]
pub use criterion::*;

#[cfg(codspeed)]
pub use codspeed_criterion_compat::*;

use rspack_sources::{
  BoxSource, CachedSource, ConcatSource, MapOptions, ReplaceSource, Source,
  SourceExt, SourceMap, SourceMapSource, SourceMapSourceOptions,
};

const HELLOWORLD_JS: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-minify/files/helloworld.js"
));
const HELLOWORLD_JS_MAP: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-minify/files/helloworld.js.map"
));
const HELLOWORLD_MIN_JS: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-minify/files/helloworld.min.js"
));
const HELLOWORLD_MIN_JS_MAP: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-minify/files/helloworld.min.js.map"
));
const BUNDLE_JS: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-rollup/files/bundle.js"
));
const BUNDLE_JS_MAP: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/transpile-rollup/files/bundle.js.map"
));
const ANTD_MIN_JS: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/antd-mini/antd.min.js"
));
const ANTD_MIN_JS_MAP: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/benches/fixtures/antd-mini/antd.min.js.map"
));

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

  let concat = ConcatSource::new([sms_minify, sms_rollup]);

  b.iter(|| {
    concat
      .map(&MapOptions::default())
      .unwrap()
      .to_json()
      .unwrap();
  })
}

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
  let concat = ConcatSource::new([sms_minify, sms_rollup]);

  b.iter(|| {
    let json = concat
      .map(&MapOptions::default())
      .unwrap()
      .to_json()
      .unwrap();
    let _ = base64_simd::STANDARD.encode_to_string(json.as_bytes());
  })
}

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
  let concat = ConcatSource::new([sms_minify, sms_rollup]);
  let cached = CachedSource::new(concat);

  b.iter(|| {
    let json = cached
      .map(&MapOptions::default())
      .unwrap()
      .to_json()
      .unwrap();
    let _ = base64_simd::STANDARD.encode_to_string(json.as_bytes());
  })
}

fn benchmark_replace_large_minified_source(b: &mut Bencher) {
  let antd_minify = SourceMapSource::new(SourceMapSourceOptions {
    value: ANTD_MIN_JS,
    name: "antd.min.js",
    source_map: SourceMap::from_json(ANTD_MIN_JS_MAP).unwrap(),
    original_source: None,
    inner_source_map: None,
    remove_original_source: false,
  });
  let mut replace_source = ReplaceSource::new(antd_minify);
  replace_source.replace(107, 114, "exports", None);
  replace_source.replace(130, 143, "'object'", None);
  replace_source.replace(165, 172, "__webpack_require__", None);
  replace_source.replace(173, 180, "/*! react */\"./node_modules/.pnpm/react@18.2.0/node_modules/react/index.js\"", None);
  replace_source.replace(183, 190, "__webpack_require__", None);
  replace_source.replace(191, 202, "/*! react-dom */\"./node_modules/.pnpm/react-dom@18.2.0_react@18.2.0/node_modules/react-dom/index.js\"", None);
  replace_source.replace(205, 212, "__webpack_require__", None);
  replace_source.replace(213, 220, "/*! dayjs */\"./node_modules/.pnpm/dayjs@1.11.10/node_modules/dayjs/dayjs.min.js\"", None);
  replace_source.replace(363, 370, "exports", None);
  replace_source.replace(373, 385, "exports.antd", None);
  replace_source.replace(390, 397, "__webpack_require__", None);
  replace_source.replace(398, 405, "/*! react */\"./node_modules/.pnpm/react@18.2.0/node_modules/react/index.js\"", None);
  replace_source.replace(408, 415, "__webpack_require__", None);
  replace_source.replace(416, 427, "/*! react-dom */\"./node_modules/.pnpm/react-dom@18.2.0_react@18.2.0/node_modules/react-dom/index.js\"", None);
  replace_source.replace(430, 437, "__webpack_require__", None);
  replace_source.replace(438, 445, "/*! dayjs */\"./node_modules/.pnpm/dayjs@1.11.10/node_modules/dayjs/dayjs.min.js\"", None);
  replace_source.replace(494, 498, "this", None);

  b.iter(|| {
    replace_source.map(&MapOptions::default());
  });
}

fn benchmark_source_for_replace_large_minified_source_with_cache(b: &mut Bencher) {
  let antd_minify = SourceMapSource::new(SourceMapSourceOptions {
    value: ANTD_MIN_JS,
    name: "antd.min.js",
    source_map: SourceMap::from_json(ANTD_MIN_JS_MAP).unwrap(),
    original_source: None,
    inner_source_map: None,
    remove_original_source: false,
  });
  let mut replace_source = ReplaceSource::new(antd_minify);
  replace_source.replace(107, 114, "exports", None);
  replace_source.replace(130, 143, "'object'", None);
  replace_source.replace(165, 172, "__webpack_require__", None);
  replace_source.replace(173, 180, "/*! react */\"./node_modules/.pnpm/react@18.2.0/node_modules/react/index.js\"", None);
  replace_source.replace(183, 190, "__webpack_require__", None);
  replace_source.replace(191, 202, "/*! react-dom */\"./node_modules/.pnpm/react-dom@18.2.0_react@18.2.0/node_modules/react-dom/index.js\"", None);
  replace_source.replace(205, 212, "__webpack_require__", None);
  replace_source.replace(213, 220, "/*! dayjs */\"./node_modules/.pnpm/dayjs@1.11.10/node_modules/dayjs/dayjs.min.js\"", None);
  replace_source.replace(363, 370, "exports", None);
  replace_source.replace(373, 385, "exports.antd", None);
  replace_source.replace(390, 397, "__webpack_require__", None);
  replace_source.replace(398, 405, "/*! react */\"./node_modules/.pnpm/react@18.2.0/node_modules/react/index.js\"", None);
  replace_source.replace(408, 415, "__webpack_require__", None);
  replace_source.replace(416, 427, "/*! react-dom */\"./node_modules/.pnpm/react-dom@18.2.0_react@18.2.0/node_modules/react-dom/index.js\"", None);
  replace_source.replace(430, 437, "__webpack_require__", None);
  replace_source.replace(438, 445, "/*! dayjs */\"./node_modules/.pnpm/dayjs@1.11.10/node_modules/dayjs/dayjs.min.js\"", None);
  replace_source.replace(494, 498, "this", None);
  let replace_source = replace_source.boxed();

  let concat_source = ConcatSource::new(vec![replace_source.clone(), replace_source]);
  let cached = CachedSource::new(concat_source);

  b.iter(|| {
    cached.source();
  });
}

fn benchmark_concat_generate_string_with_cache_as_key(b: &mut Bencher) {
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
  let cached = CachedSource::new(concat).boxed();

  b.iter(|| {
    let mut m = HashMap::<BoxSource, ()>::new();
    m.insert(cached.clone(), ());
    let _ = black_box(|| m.get(&cached));
    let _ = black_box(|| m.get(&cached));
  })
}

fn benchmark_concat_generate_string_as_key(b: &mut Bencher) {
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
  let concat = ConcatSource::new([sms_minify, sms_rollup]).boxed();

  b.iter(|| {
    let mut m = HashMap::<BoxSource, ()>::new();
    m.insert(concat.clone(), ());
    let _ = black_box(|| m.get(&concat));
    let _ = black_box(|| m.get(&concat));
  })
}

fn bench_rspack_sources(criterion: &mut Criterion) {
  let mut group = criterion.benchmark_group("rspack_sources");
  group.bench_function(
    "concat_generate_base64_with_cache",
    benchmark_concat_generate_base64_with_cache,
  );
  group
    .bench_function("concat_generate_base64", benchmark_concat_generate_base64);
  group.bench_function(
    "concat_generate_string_with_cache",
    benchmark_concat_generate_string_with_cache,
  );
  group
    .bench_function("concat_generate_string", benchmark_concat_generate_string);
  group.bench_function(
    "replace_large_minified_source",
    benchmark_replace_large_minified_source,
  );
  group.bench_function(
    "concat_generate_string_with_cache_as_key",
    benchmark_concat_generate_string_with_cache_as_key,
  );
  group.bench_function(
    "concat_generate_string_as_key",
    benchmark_concat_generate_string_as_key,
  );
  group.bench_function(
    "source_for_replace_large_minified_source_with_cache",
    benchmark_source_for_replace_large_minified_source_with_cache,
  );
  group.finish();
}

criterion_group!(rspack_sources, bench_rspack_sources);
criterion_main!(rspack_sources);
