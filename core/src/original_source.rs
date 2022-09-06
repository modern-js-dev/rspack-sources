use std::borrow::Cow;

use sourcemap::SourceMapBuilder;

use crate::{MapOptions, Source, SourceMap};

pub struct OriginalSource {
  source: String,
  name: String,
}

impl OriginalSource {
  pub fn new(source: impl Into<String>, name: impl Into<String>) -> Self {
    Self {
      source: source.into(),
      name: name.into(),
    }
  }

  pub fn get_name(&self) -> &str {
    &self.name
  }
}

impl Source for OriginalSource {
  fn map(&self, options: &MapOptions) -> Option<SourceMap> {
    let columns = options.columns;

    let mut sm_builder = SourceMapBuilder::new(None);
    let source_id = sm_builder.add_source(&self.name);
    if options.include_source_contents {
      sm_builder.set_source_contents(source_id, Some(&self.source));
    }

    if columns {
      let mut line = 0;
      let mut col = 0;
      self.source.chars().into_iter().for_each(|c| {
        if col == 0 {
          sm_builder.add(line, 0, line, 0, Some(self.name.as_str()), None);
        }

        match c {
          '\n' => {
            line += 1;
            col = 0;
          }
          ';' | '}' => {
            col += 1;
            sm_builder.add(line, col, line, col, Some(self.name.as_str()), None);
          }
          '{' => {
            sm_builder.add(line, col, line, col, Some(self.name.as_str()), None);
            col += 1;
          }
          _ => {
            col += 1;
          }
        }
      });
    } else {
      let line = self.source.split('\n').count();

      for index in 0..line {
        sm_builder.add(
          index as u32,
          0,
          index as u32,
          0,
          Some(self.name.as_str()),
          None,
        );
      }
    }

    Some(SourceMap::from_inner(sm_builder.into_sourcemap()))
  }

  fn buffer(&self) -> &[u8] {
    self.source.as_bytes()
  }

  fn source(&self) -> Cow<str> {
    Cow::Borrowed(&self.source)
  }

  fn size(&self) -> usize {
    self.source.len()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn original_source() {
    let original_source = OriginalSource::new(
      r#"import { createElement } from "react";
import { render } from "react-dom";
const div = createElement("div", null, {});
render(div, document.getElementById("app"));
"#,
      "app.js",
    );

    let source_map = original_source
      .map(&MapOptions {
        columns: true,
        ..Default::default()
      })
      .expect("should generate");

    let source_map_string = source_map.to_string();
    assert_eq!(source_map_string, "");
    assert_eq!(original_source.source(), "{\"version\":3,\"sources\":[\"app.js\"],\"sourcesContent\":[\"import { createElement } from \\\"react\\\";\\nimport { render } from \\\"react-dom\\\";\\nconst div = createElement(\\\"div\\\", null, {});\\nrender(div, document.getElementById(\\\"app\\\"));\\n\"],\"names\":[],\"mappings\":\"AAAA,OAAO,iBAAiB,cAAc;AACtC,OAAO,UAAU,kBAAkB;AACnC,uCAAuC,EAAE,EAAE;AAC3C,4CAA4C\"}");
  }

  #[test]
  fn should_handle_multiline_string() {
    let source = OriginalSource::new("Line1\n\nLine3\n", "file.js");
    let result_text = source.source();
    let result_map = source
      .map(&MapOptions {
        columns: true,
        ..Default::default()
      })
      .unwrap();
    let result_map_inner = result_map.inner();
    let result_list_map = source
      .map(&MapOptions {
        columns: false,
        ..Default::default()
      })
      .unwrap();
    let result_list_map_inner = result_list_map.inner();

    assert_eq!(result_text, "Line1\n\nLine3\n");
    assert_eq!(
      result_map_inner.sources().collect::<Vec<_>>(),
      vec!["file.js"]
    );
    assert_eq!(
      result_list_map_inner.sources().collect::<Vec<_>>(),
      result_map_inner.sources().collect::<Vec<_>>(),
    );
    assert_eq!(
      result_map_inner.source_contents().collect::<Vec<_>>(),
      vec![Some("Line1\n\nLine3\n")],
    );
    assert_eq!(
      result_list_map_inner.source_contents().collect::<Vec<_>>(),
      result_map_inner.source_contents().collect::<Vec<_>>(),
    );
    drop(result_map_inner);
    drop(result_list_map_inner);
    assert_eq!(result_map.to_string(), "{\"version\":3,\"sources\":[\"file.js\"],\"sourcesContent\":[\"Line1\\n\\nLine3\\n\"],\"names\":[],\"mappings\":\"AAAA;AACA;AACA\"}");
    assert_eq!(result_list_map.to_string(), "{\"version\":3,\"sources\":[\"file.js\"],\"sourcesContent\":[\"Line1\\n\\nLine3\\n\"],\"names\":[],\"mappings\":\"AAAA;AACA;AACA;AACA\"}");
  }

  #[test]
  fn should_handle_empty_string() {
    let source = OriginalSource::new("", "file.js");
    let result_text = source.source();
    let result_map = source
      .map(&MapOptions {
        columns: true,
        ..Default::default()
      })
      .unwrap();
    let result_list_map = source
      .map(&MapOptions {
        columns: false,
        ..Default::default()
      })
      .unwrap();

    assert_eq!(result_text, "");
    assert_eq!(result_map.to_string(), "{\"version\":3,\"sources\":[\"file.js\"],\"sourcesContent\":[\"\"],\"names\":[],\"mappings\":\"\"}");
    assert_eq!(result_list_map.to_string(), "{\"version\":3,\"sources\":[\"file.js\"],\"sourcesContent\":[\"\"],\"names\":[],\"mappings\":\"AAAA\"}");
  }

  #[test]
  fn should_return_the_correct_size_for_unicode_files() {
    let source = OriginalSource::new("😋", "file.js");
    assert_eq!(source.size(), 4);
  }

  #[test]
  fn should_split_code_into_statements() {
    let input = "if (hello()) { world(); hi(); there(); } done();\nif (hello()) { world(); hi(); there(); } done();";
    let source = OriginalSource::new(input, "file.js");
    assert_eq!(source.source(), input);
    assert_eq!(source.map(&MapOptions::default()).unwrap().to_string(), "{\"version\":3,\"sources\":[\"file.js\"],\"sourcesContent\":[\"if (hello()) { world(); hi(); there(); } done();\\nif (hello()) { world(); hi(); there(); } done();\"],\"names\":[],\"mappings\":\"AAAA,aAAa,UAAU,MAAM,SAAS,EAAE,QAAQ;AAChD,aAAa,UAAU,MAAM,SAAS,EAAE,QAAQ\"}");
  }
}
