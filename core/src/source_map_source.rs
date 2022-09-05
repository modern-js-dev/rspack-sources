use parcel_sourcemap::{Mapping, OriginalLocation, SourceMap};
use smol_str::SmolStr;

use crate::{Error, MapOptions, Source};

#[derive(Debug, Clone)]
pub struct SourceMapSource {
  source_code: SmolStr,
  name: SmolStr,
  source_map: SourceMap,
  original_source: Option<SmolStr>,
  inner_source_map: Option<SourceMap>,
}

pub struct SourceMapSourceSliceOptions<'a> {
  pub source_code: &'a [u8],
  pub name: String,
  pub source_map: SourceMap,
  pub original_source: Option<&'a [u8]>,
  pub inner_source_map: Option<SourceMap>,
}

pub struct SourceMapSourceOptions {
  pub source_code: String,
  pub name: String,
  pub source_map: SourceMap,
  pub original_source: Option<String>,
  pub inner_source_map: Option<SourceMap>,
}

impl SourceMapSource {
  pub fn new(options: SourceMapSourceOptions) -> Self {
    let SourceMapSourceOptions {
      source_code,
      name,
      source_map,
      original_source,
      inner_source_map,
    } = options;

    let original_source: Option<SmolStr> = original_source.map(Into::into);

    Self {
      source_code: source_code.into(),
      name: name.into(),
      source_map,
      original_source,
      inner_source_map,
    }
  }

  pub fn from_slice(options: SourceMapSourceSliceOptions) -> Result<Self, Error> {
    let SourceMapSourceSliceOptions {
      source_code,
      name,
      source_map,
      original_source,
      inner_source_map,
    } = options;

    let original_source = if let Some(original_source) = original_source {
      Some(String::from_utf8(original_source.to_vec())?)
    } else {
      None
    };

    let original_source: Option<SmolStr> = original_source.map(Into::into);

    Ok(Self {
      source_code: String::from_utf8(source_code.to_vec())?.into(),
      name: name.into(),
      source_map,
      original_source,
      inner_source_map,
    })
  }

  fn ensure_source_map(&mut self) {
    let current_file_name = &self.name;
    let source_idx = self
      .source_map
      .get_sources()
      .into_iter()
      .enumerate()
      .find_map(|(idx, source)| {
        if source == current_file_name {
          Some(idx)
        } else {
          None
        }
      });

    if let Some(source_idx) = source_idx
      && let Some(original_source) = &self.original_source
      && self.source_map.get_source_content(source_idx as u32).is_ok() {
      self.source_map.set_source_content(
        source_idx,
        original_source,
      ).unwrap();
    }
  }

  // fn find_original_token(&self, token: &Mapping) -> (Mapping, Option<&str>) {
  //   let load_source_contents = || {
  //     token
  //       .original
  //       .map(|original| self.source_map.get_source_content(original.source).unwrap())
  //   };

  //   if let Some(inner_source_map) = &self.inner_source_map {
  //     if let Some(original) = token.original {
  //       let source = original.source;
  //       let src_line = original.original_line;
  //       let src_col = original.original_column;

  //       if inner_source_map.get_file() == source {
  //         if let Some(original_token) = inner_source_map.lookup_token(src_line, src_col) {
  //           (
  //             original_token,
  //             inner_source_map.get_source_contents(original_token.get_src_id()),
  //           )
  //         } else {
  //           (*token, load_source_contents())
  //         }
  //       } else {
  //         (*token, load_source_contents())
  //       }
  //     }
  //   } else {
  //     (*token, load_source_contents())
  //   }
  // }

  // #[tracing::instrument(skip_all)]
  // fn remap_with_inner_sourcemap(&mut self, gen_map_option: &MapOptions) -> Option<SourceMap> {
  //   let mut source_map_builder = SourceMap::new("/");

  //   if self.inner_source_map.is_some() {
  //     let source_map = &self.source_map;
  //     source_map.get_mappings().into_iter().for_each(|token| {
  //       let (original_token, source_content) = self.find_original_token(&token);

  //       source_map_builder.add_mapping(
  //         token.generated_line,
  //         token.generated_column,
  //         token.original,
  //       );
  //       // let raw_token = source_map_builder.add(
  //       //   token.get_dst_line(),
  //       //   token.get_dst_col(),
  //       //   original_token.get_src_line(),
  //       //   original_token.get_src_col(),
  //       //   original_token.get_source(),
  //       //   original_token.get_name(),
  //       // );

  //       // if gen_map_option.include_source_contents && !self.remove_original_source {
  //       //   source_map_builder.set_source_contents(raw_token.src_id, source_content);
  //       // }
  //     });

  //     return Some(source_map_builder);
  //   }

  //   None
  // }
}

impl Source for SourceMapSource {
  #[tracing::instrument(skip_all)]
  fn source(&mut self) -> String {
    self.source_code.to_string()
  }

  // #[tracing::instrument(skip_all)]
  // fn map(&mut self, option: &MapOptions) -> Option<SourceMap> {
  //   self.remap_with_inner_sourcemap(option)
  // }

  #[tracing::instrument(skip_all)]
  fn map(&mut self, option: &MapOptions) -> Option<SourceMap> {
    self.ensure_source_map();
    let mut sm = self.source_map.clone();
    if let Some(inner_source_map) = &mut self.inner_source_map {
      sm.extends(inner_source_map).unwrap();
    }
    Some(sm)
  }

  #[tracing::instrument]
  fn size(&self) -> usize {
    self.source_code.len()
  }
}

#[cfg(test)]
mod tests {
  use crate::{source::BoxSource, ConcatSource, OriginalSource, RawSource};

  use super::*;

  #[test]
  fn map_correctly() {
    let inner_source_code = "Hello World\nis a test string\n";
    let mut inner_source = ConcatSource::new([
      Box::new(OriginalSource::new(inner_source_code, "hello-world.txt")) as BoxSource,
      Box::new(OriginalSource::new("Translate: ", "header.txt")),
      Box::new(RawSource::new("Other text")),
    ]);

    let source_r_code = "Translated: Hallo Welt\nist ein test Text\nAnderer Text";
    let source_r_map = SourceMap::from_json(
      "/",
      r#"{
        "version": 3,
        "sources": [ "text" ],
        "names": [ "Hello", "World", "nope" ],
        "mappings": "YAAAA,K,CAAMC;AACNC,O,MAAU;AACC,O,CAAM",
        "file": "translated.txt",
        "sourcesContent": [ "Hello World\nis a test string\n" ]
      }"#,
    )
    .unwrap();
    let mut source_map_source1 = SourceMapSource::new(SourceMapSourceOptions {
      source_code: source_r_code.to_string(),
      name: "text".to_string(),
      source_map: source_r_map.clone(),
      original_source: Some(inner_source.source()),
      inner_source_map: inner_source.map(&MapOptions::default()),
    });
    let mut source_map_source2 = SourceMapSource::new(SourceMapSourceOptions {
      source_code: source_r_code.to_string(),
      name: "text".to_string(),
      source_map: source_r_map,
      original_source: Some(inner_source.source()),
      inner_source_map: inner_source.map(&MapOptions::default()),
    });

    let expected_content = "Translated: Hallo Welt\nist ein test Text\nAnderer Text";
    assert_eq!(source_map_source1.source(), expected_content);
    assert_eq!(source_map_source2.source(), expected_content);
    assert_eq!(
      source_map_source1
        .map(&MapOptions::default())
        .unwrap()
        .to_json(None)
        .unwrap(),
      "{\"version\":3,\"sourceRoot\":null,\"mappings\":\"YCAA,K,CAAA;AACA,O,MAAA;ACDA,O,CAAA\",\"sources\":[\"text\",\"hello-world.txt\",\"header.txt\"],\"sourcesContent\":[\"Hello World\\nis a test string\\nTranslate: Other text\",\"Hello World\\nis a test string\\n\",\"Translate: \"],\"names\":[\"Hello\",\"World\",\"nope\"]}",
    );
  }

  #[test]
  fn should_handle_es6_promise_correctly() {
    let code = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/tests/fixtures/es6-promise.js"
    ));
    let map = SourceMap::from_json(
      "/",
      include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/es6-promise.map"
      )),
    )
    .unwrap();
    let inner = SourceMapSource::new(SourceMapSourceOptions {
      source_code: code.to_string(),
      name: "es6-promise.js".to_string(),
      source_map: map,
      original_source: None,
      inner_source_map: None,
    });
    let mut source = ConcatSource::new([inner.clone(), inner]);
    assert_eq!(source.source(), format!("{}{}", &code, &code));
  }
}

// impl From<SourceMapSource> for CachedSource<SourceMapSource> {
//   fn from(source_map: SourceMapSource) -> Self {
//     Self::new(source_map)
//   }
// }

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

  let mut source_map_source = SourceMapSource::new(SourceMapSourceOptions {
    source_code: transformed_code.to_string(),
    name: "helloworld.min.js".into(),
    source_map: SourceMap::from_json("/", transformed_map).unwrap(),
    original_source: Some(original_code.to_string()),
    inner_source_map: Some(SourceMap::from_json("/", original_map).unwrap()),
  });

  let mut new_source_map = source_map_source.map(&Default::default()).expect("failed");
  let token = new_source_map.find_closest_mapping(15, 47).expect("failed");

  println!("{}", new_source_map.to_json(None).unwrap());
  println!("{}", source_map_source.source());
  assert_eq!(
    new_source_map
      .get_source(token.original.unwrap().source)
      .unwrap(),
    "helloworld.mjs"
  );
  assert_eq!(token.original.unwrap().original_column, 20);
  assert_eq!(token.original.unwrap().original_line, 18);
  assert_eq!(
    new_source_map
      .get_name(token.original.unwrap().name.unwrap())
      .unwrap(),
    "alert"
  );
}
