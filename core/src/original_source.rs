use parcel_sourcemap::{OriginalLocation, SourceMap};
use smol_str::SmolStr;

use crate::{MapOptions, Source};

pub struct OriginalSource {
  source_code: SmolStr,
  name: SmolStr,
}

impl OriginalSource {
  pub fn new(source_code: &str, name: &str) -> Self {
    Self {
      source_code: source_code.into(),
      name: name.into(),
    }
  }
}

impl Source for OriginalSource {
  fn map(&mut self, option: &MapOptions) -> Option<SourceMap> {
    let columns = option.columns;

    let mut sm_builder = SourceMap::new("/");
    let source_id = sm_builder.add_source(&self.name);
    if option.include_source_contents {
      sm_builder.set_source_content(source_id as usize, &self.source_code);
    }

    if columns {
      let mut line = 0;
      let mut col = 0;
      self.source_code.chars().into_iter().for_each(|c| {
        if col == 0 {
          sm_builder.add_mapping(
            line,
            0,
            Some(OriginalLocation {
              original_line: line,
              original_column: 0,
              source: source_id,
              name: None,
            }),
          );
        }

        match c {
          '\n' => {
            line += 1;
            col = 0;
          }
          ';' | '}' => {
            col += 1;
            sm_builder.add_mapping(
              line,
              col,
              Some(OriginalLocation {
                original_line: line,
                original_column: col,
                source: source_id,
                name: None,
              }),
            );
          }
          '{' => {
            sm_builder.add_mapping(
              line,
              col,
              Some(OriginalLocation {
                original_line: line,
                original_column: col,
                source: source_id,
                name: None,
              }),
            );
            col += 1;
          }
          _ => {
            col += 1;
          }
        }
      });
    } else {
      let line = self.source_code.split('\n').count();

      for index in 0..line {
        sm_builder.add_mapping(
          index as u32,
          0,
          Some(OriginalLocation {
            original_line: index as u32,
            original_column: 0,
            source: source_id,
            name: None,
          }),
        );
      }
    }

    Some(sm_builder)
  }

  fn source(&mut self) -> String {
    self.source_code.to_string()
  }

  fn size(&self) -> usize {
    self.source_code.len()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn get_mappings_string(source_map: &mut SourceMap) -> String {
    let mut buf = Vec::new();
    source_map.write_vlq(&mut buf).unwrap();
    String::from_utf8(buf).unwrap()
  }

  #[test]
  fn original_source() {
    let mut original_source = OriginalSource::new(
      r#"import { createElement } from "react";
  import { render } from "react-dom";
  const div = createElement("div", null, {});
  render(div, document.getElementById("app"));"#,
      "app.js",
    );

    let mut source_map = original_source
      .map(&MapOptions {
        columns: true,
        ..Default::default()
      })
      .expect("should generate");

    let source_map_string = source_map.to_json(None).unwrap();
    println!("{}", source_map_string);
    println!("{}", original_source.source());
  }

  #[test]
  fn original_source_2() {
    let mut original_source = OriginalSource::new(
      "console.log('test');\nconsole.log('test2');\n",
      "console.js",
    );

    let mut source_map = original_source
      .map(&MapOptions {
        columns: true,
        ..Default::default()
      })
      .expect("should generate");

    let source_map_string = source_map.to_json(None).unwrap();
    println!("{}", source_map_string);
    println!("{}", original_source.source());
  }

  #[test]
  fn should_handle_multiline_string() {
    let mut source = OriginalSource::new("Line1\n\nLine3\n", "file.js");
    let result_text = source.source();
    let mut result_map = source
      .map(&MapOptions {
        columns: true,
        ..Default::default()
      })
      .unwrap();
    let mut result_list_map = source
      .map(&MapOptions {
        columns: false,
        ..Default::default()
      })
      .unwrap();

    assert_eq!(result_text, "Line1\n\nLine3\n");
    assert_eq!(result_map.get_sources(), &vec!["file.js"]);
    assert_eq!(result_list_map.get_sources(), result_map.get_sources());
    assert_eq!(result_map.get_sources_content(), &vec!["Line1\n\nLine3\n"]);
    assert_eq!(
      result_list_map.get_sources_content(),
      result_map.get_sources_content()
    );
    assert_eq!(get_mappings_string(&mut result_map), "AAAA;AACA;AACA");
    assert_eq!(
      get_mappings_string(&mut result_list_map),
      "AAAA;AACA;AACA;AACA"
    );
    assert_eq!(result_map.to_json(None).unwrap(), "{\"version\":3,\"sourceRoot\":null,\"mappings\":\"AAAA;AACA;AACA\",\"sources\":[\"file.js\"],\"sourcesContent\":[\"Line1\\n\\nLine3\\n\"],\"names\":[]}");
    assert_eq!(result_list_map.to_json(None).unwrap(), "{\"version\":3,\"sourceRoot\":null,\"mappings\":\"AAAA;AACA;AACA;AACA\",\"sources\":[\"file.js\"],\"sourcesContent\":[\"Line1\\n\\nLine3\\n\"],\"names\":[]}");
  }

  #[test]
  fn should_handle_empty_string() {
    let mut source = OriginalSource::new("", "file.js");
    let result_text = source.source();
    let mut result_map = source
      .map(&MapOptions {
        columns: true,
        ..Default::default()
      })
      .unwrap();
    let mut result_list_map = source
      .map(&MapOptions {
        columns: false,
        ..Default::default()
      })
      .unwrap();

    assert_eq!(result_text, "");
    assert_eq!(get_mappings_string(&mut result_map), "");
    assert_eq!(get_mappings_string(&mut result_list_map), "AAAA");
    assert_eq!(result_map.to_json(None).unwrap(), "{\"version\":3,\"sourceRoot\":null,\"mappings\":\"\",\"sources\":[\"file.js\"],\"sourcesContent\":[\"\"],\"names\":[]}");
    assert_eq!(result_list_map.to_json(None).unwrap(), "{\"version\":3,\"sourceRoot\":null,\"mappings\":\"AAAA\",\"sources\":[\"file.js\"],\"sourcesContent\":[\"\"],\"names\":[]}");
  }

  #[test]
  fn should_return_the_correct_size_for_binary_files() {
    let source = OriginalSource::new(&String::from_utf8(vec![0; 256]).unwrap(), "file.wasm");
    assert_eq!(source.size(), 256);
  }

  #[test]
  fn should_return_the_correct_size_for_unicode_files() {
    let source = OriginalSource::new("😋", "file.js");
    assert_eq!(source.size(), 4);
  }

  #[test]
  fn should_split_code_into_statements() {
    let input = "if (hello()) { world(); hi(); there(); } done();\nif (hello()) { world(); hi(); there(); } done();";
    let mut source = OriginalSource::new(input, "file.js");
    assert_eq!(source.source(), input);
    assert_eq!(
      get_mappings_string(
        &mut source
          .map(&MapOptions {
            columns: true,
            ..Default::default()
          })
          .unwrap()
      ),
      "AAAA,aAAa,UAAU,MAAM,SAAS,EAAE,QAAQ;AAChD,aAAa,UAAU,MAAM,SAAS,EAAE,QAAQ"
    );
    assert_eq!(
      get_mappings_string(
        &mut source
          .map(&MapOptions {
            columns: false,
            ..Default::default()
          })
          .unwrap()
      ),
      "AAAA;AACA"
    );
  }
}
