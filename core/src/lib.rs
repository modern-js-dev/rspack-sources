#![deny(clippy::all)]

mod result;

mod cached_source;
mod concat_source;
mod original_source;
mod raw_source;
mod source;
mod source_map_source;
mod utils;

pub use cached_source::CachedSource;
pub use concat_source::ConcatSource;
pub use original_source::OriginalSource;
pub use parcel_sourcemap::{self, SourceMap};
pub use raw_source::RawSource;
pub use result::{Error, RspackSourcesError};
pub use source::{MapOptions, Source};
pub use source_map_source::{SourceMapSource, SourceMapSourceOptions, SourceMapSourceSliceOptions};

#[test]
fn t() {
  use rayon::prelude::*;

  let s1 = OriginalSource::new("console.log('hah')", "file.js");
  let s2 = OriginalSource::new("console.log('hah')", "file.js");
  let s3 = OriginalSource::new("console.log('hah')", "file.js");
  let s4 = OriginalSource::new("console.log('hah')", "file.js");
  let mut ss = vec![s1, s2, s3, s4];

  let ss: Vec<_> = ss
    .par_iter_mut()
    .map(|s| {
      s.map(&MapOptions {
        columns: true,
        ..Default::default()
      })
      .unwrap()
      .to_json(None)
      .unwrap()
    })
    .collect();
  dbg!(ss);
}
