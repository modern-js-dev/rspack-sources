use arrayvec::ArrayVec;
use rustc_hash::FxHashMap as HashMap;
use std::{
  borrow::{BorrowMut, Cow},
  cell::{OnceCell, RefCell},
  rc::Rc,
  sync::Arc,
};

use crate::{
  source::{Mapping, OriginalLocation},
  vlq::{decode, encode},
  with_indices::WithIndices,
  MapOptions, Source, SourceMap,
};

type ArcStr = Arc<str>;
// Adding this type because sourceContentLine not happy
type InnerSourceContentLine =
  RefCell<HashMap<i64, Option<Rc<Vec<WithIndices<ArcStr>>>>>>;

pub fn get_map<S: StreamChunks>(
  stream: &S,
  options: &MapOptions,
) -> Option<SourceMap> {
  let mut mappings = Vec::new();
  let mut sources: Vec<Cow<'static, str>> = Vec::new();
  let mut sources_content: Vec<Cow<'static, str>> = Vec::new();
  let mut names: Vec<Cow<'static, str>> = Vec::new();
  stream.stream_chunks(
    &MapOptions {
      columns: options.columns,
      final_source: true,
    },
    // on_chunk
    &mut |_, mapping| {
      mappings.push(mapping);
    },
    // on_source
    &mut |source_index, source: &str, source_content: Option<&str>| {
      let source_index = source_index as usize;
      if sources.len() <= source_index {
        sources.resize(source_index + 1, Cow::Borrowed(""));
      }
      sources[source_index] = source.to_string().into();
      if let Some(source_content) = source_content {
        if sources_content.len() <= source_index {
          sources_content.resize(source_index + 1, Cow::Borrowed(""));
        }
        sources_content[source_index] = source_content.to_string().into();
      }
    },
    // on_name
    &mut |name_index, name: &str| {
      let name_index = name_index as usize;
      if names.len() <= name_index {
        names.resize(name_index + 1, Cow::Borrowed(""));
      }
      names[name_index] = name.to_string().into();
    },
  );
  let mappings = encode_mappings(&mappings, options);
  (!mappings.is_empty())
    .then(|| SourceMap::new(None, mappings, sources, sources_content, names))
}

/// [StreamChunks] abstraction, see [webpack-sources source.streamChunks](https://github.com/webpack/webpack-sources/blob/9f98066311d53a153fdc7c633422a1d086528027/lib/helpers/streamChunks.js#L13).
pub trait StreamChunks {
  /// [StreamChunks] abstraction
  fn stream_chunks(
    &self,
    options: &MapOptions,
    on_chunk: OnChunk,
    on_source: OnSource,
    on_name: OnName,
  ) -> GeneratedInfo;
}

/// [OnChunk] abstraction, see [webpack-sources onChunk](https://github.com/webpack/webpack-sources/blob/9f98066311d53a153fdc7c633422a1d086528027/lib/helpers/streamChunks.js#L13).
pub type OnChunk<'a> = &'a mut dyn FnMut(Option<&str>, Mapping);

/// [OnSource] abstraction, see [webpack-sources onSource](https://github.com/webpack/webpack-sources/blob/9f98066311d53a153fdc7c633422a1d086528027/lib/helpers/streamChunks.js#L13).
pub type OnSource<'a> = &'a mut dyn FnMut(u32, &str, Option<&str>);

/// [OnName] abstraction, see [webpack-sources onName](https://github.com/webpack/webpack-sources/blob/9f98066311d53a153fdc7c633422a1d086528027/lib/helpers/streamChunks.js#L13).
pub type OnName<'a> = &'a mut dyn FnMut(u32, &str);

/// Default stream chunks behavior impl, see [webpack-sources streamChunks](https://github.com/webpack/webpack-sources/blob/9f98066311d53a153fdc7c633422a1d086528027/lib/helpers/streamChunks.js#L15-L35).
pub fn stream_chunks_default<S: Source>(
  source: &S,
  options: &MapOptions,
  on_chunk: OnChunk,
  on_source: OnSource,
  on_name: OnName,
) -> GeneratedInfo {
  if let Some(map) = source.map(options) {
    stream_chunks_of_source_map(
      &source.source(),
      &map,
      on_chunk,
      on_source,
      on_name,
      options,
    )
  } else {
    stream_chunks_of_raw_source(
      &source.source(),
      options,
      on_chunk,
      on_source,
      on_name,
    )
  }
}

/// `GeneratedSourceInfo` abstraction, see [webpack-sources GeneratedSourceInfo](https://github.com/webpack/webpack-sources/blob/9f98066311d53a153fdc7c633422a1d086528027/lib/helpers/getGeneratedSourceInfo.js)
#[derive(Debug, PartialEq, Eq)]
pub struct GeneratedInfo {
  /// Generated line
  pub generated_line: u32,
  /// Generated column
  pub generated_column: u32,
}

pub fn decode_mappings<'b, 'a: 'b>(
  source_map: &'a SourceMap,
) -> impl Iterator<Item = Mapping> + 'b {
  SegmentIter::new(source_map.mappings())
}

pub struct SegmentIter<'a> {
  mapping_str: &'a str,
  generated_line: usize,
  generated_column: u32,
  source_index: u32,
  original_line: u32,
  original_column: u32,
  name_index: u32,
  line: &'a str,
  nums: ArrayVec<i64, 5>,
  segment_cursor: usize,
}

impl<'a> SegmentIter<'a> {
  pub fn new(mapping_str: &'a str) -> Self {
    SegmentIter {
      line: "",
      mapping_str,
      source_index: 0,
      original_line: 1,
      original_column: 0,
      name_index: 0,
      generated_line: 0,
      segment_cursor: 0,
      generated_column: 0,
      nums: ArrayVec::new(),
    }
  }

  fn next_segment(&mut self) -> Option<&'a str> {
    if self.line.is_empty() {
      loop {
        match self.next_line() {
          Some(line) => {
            self.generated_line += 1;
            if line.is_empty() {
              continue;
            }
            self.line = line;
            self.generated_column = 0;
            self.segment_cursor = 0;
            break;
          }
          None => return None,
        }
      }
    }

    if let Some(i) =
      memchr::memchr(b',', self.line[self.segment_cursor..].as_bytes())
    {
      let cursor = self.segment_cursor;
      self.segment_cursor = self.segment_cursor + i + 1;
      Some(&self.line[cursor..cursor + i])
    } else {
      let line = self.line;
      self.line = "";
      Some(&line[self.segment_cursor..])
    }
  }

  fn next_line(&mut self) -> Option<&'a str> {
    if self.mapping_str.is_empty() {
      return None;
    }
    match memchr::memchr(b';', self.mapping_str.as_bytes()) {
      Some(i) => {
        let temp_str = self.mapping_str;
        self.mapping_str = &self.mapping_str[i + 1..];
        Some(&temp_str[..i])
      }
      None => {
        let tem_str = self.mapping_str;
        self.mapping_str = "";
        Some(tem_str)
      }
    }
  }
}

impl<'a> Iterator for SegmentIter<'a> {
  type Item = Mapping;

  fn next(&mut self) -> Option<Self::Item> {
    match self.next_segment() {
      Some(segment) => {
        self.nums.clear();
        decode(segment, &mut self.nums).unwrap();
        self.generated_column =
          (i64::from(self.generated_column) + self.nums[0]) as u32;

        let mut src = None;
        let mut name = None;

        if self.nums.len() > 1 {
          if self.nums.len() != 4 && self.nums.len() != 5 {
            panic!("got {} segments, expected 4 or 5", self.nums.len());
          }
          self.source_index =
            (i64::from(self.source_index) + self.nums[1]) as u32;
          src = Some(self.source_index);
          self.original_line =
            (i64::from(self.original_line) + self.nums[2]) as u32;
          self.original_column =
            (i64::from(self.original_column) + self.nums[3]) as u32;

          if self.nums.len() > 4 {
            self.name_index =
              (i64::from(self.name_index) + self.nums[4]) as u32;
            name = Some(self.name_index);
          }
        }

        Some(Mapping {
          generated_line: self.generated_line as u32,
          generated_column: self.generated_column,
          original: src.map(|src_id| OriginalLocation {
            source_index: src_id,
            original_line: self.original_line,
            original_column: self.original_column,
            name_index: name,
          }),
        })
      }
      None => None,
    }
  }
}

pub fn encode_mappings(mappings: &[Mapping], options: &MapOptions) -> String {
  if options.columns {
    encode_full_mappings(mappings)
  } else {
    encode_lines_only_mappings(mappings)
  }
}

fn encode_full_mappings(mappings: &[Mapping]) -> String {
  let mut current_line = 1;
  let mut current_column = 0;
  let mut current_original_line = 1;
  let mut current_original_column = 0;
  let mut current_source_index = 0;
  let mut current_name_index = 0;
  let mut active_mapping = false;
  let mut active_name = false;
  let mut initial = true;

  mappings.iter().fold(
    String::with_capacity(mappings.len() * 6),
    |mut acc, mapping| {
      if active_mapping && current_line == mapping.generated_line {
        // A mapping is still active
        if mapping.original.is_some_and(|original| {
          original.source_index == current_source_index
            && original.original_line == current_original_line
            && original.original_column == current_original_column
            && !active_name
            && original.name_index.is_none()
        }) {
          // avoid repeating the same original mapping
          return acc;
        }
      } else {
        // No mapping is active
        if mapping.original.is_none() {
          // avoid writing unnecessary generated mappings
          return acc;
        }
      }

      if current_line < mapping.generated_line {
        (0..mapping.generated_line - current_line).for_each(|_| acc.push(';'));
        current_line = mapping.generated_line;
        current_column = 0;
        initial = false;
      } else if initial {
        initial = false;
      } else {
        acc.push(',');
      }

      encode(&mut acc, mapping.generated_column, current_column);
      current_column = mapping.generated_column;
      if let Some(original) = &mapping.original {
        active_mapping = true;
        if original.source_index == current_source_index {
          acc.push('A');
        } else {
          encode(&mut acc, original.source_index, current_source_index);
          current_source_index = original.source_index;
        }
        encode(&mut acc, original.original_line, current_original_line);
        current_original_line = original.original_line;
        if original.original_column == current_original_column {
          acc.push('A');
        } else {
          encode(&mut acc, original.original_column, current_original_column);
          current_original_column = original.original_column;
        }
        if let Some(name_index) = original.name_index {
          encode(&mut acc, name_index, current_name_index);
          current_name_index = name_index;
          active_name = true;
        } else {
          active_name = false;
        }
      } else {
        active_mapping = false;
      }
      acc
    },
  )
}

fn encode_lines_only_mappings(mappings: &[Mapping]) -> String {
  let mut last_written_line = 0;
  let mut current_line = 1;
  let mut current_source_index = 0;
  let mut current_original_line = 1;
  mappings.iter().fold(String::new(), |acc, mapping| {
    if let Some(original) = &mapping.original {
      if last_written_line == mapping.generated_line {
        // avoid writing multiple original mappings per line
        return acc;
      }
      let mut out = String::new();
      last_written_line = mapping.generated_line;
      if mapping.generated_line == current_line + 1 {
        current_line = mapping.generated_line;
        if original.source_index == current_source_index {
          if original.original_line == current_original_line + 1 {
            current_original_line = original.original_line;
            out.push_str(";AACA");
            return acc + &out;
          } else {
            out.push_str(";AA");
            encode(&mut out, original.original_line, current_original_line);
            current_original_line = original.original_line;
            out.push('A');
            return acc + &out;
          }
        } else {
          out.push_str(";A");
          encode(&mut out, original.source_index, current_source_index);
          current_source_index = original.source_index;
          encode(&mut out, original.original_line, current_original_line);
          current_original_line = original.original_line;
          out.push('A');
          return acc + &out;
        }
      } else {
        (0..mapping.generated_line - current_line).for_each(|_| out.push(';'));
        current_line = mapping.generated_line;
        if original.source_index == current_source_index {
          if original.original_line == current_original_line + 1 {
            current_original_line = original.original_line;
            out.push_str("AACA");
            return acc + &out;
          } else {
            out.push_str("AA");
            encode(&mut out, original.original_line, current_original_line);
            current_original_line = original.original_line;
            out.push('A');
            return acc + &out;
          }
        } else {
          out.push('A');
          encode(&mut out, original.source_index, current_source_index);
          current_source_index = original.source_index;
          encode(&mut out, original.original_line, current_original_line);
          current_original_line = original.original_line;
          out.push('A');
          return acc + &out;
        }
      }
    }
    // avoid writing generated mappings at all
    acc
  })
}

pub struct PotentialTokens<'a> {
  bytes: &'a [u8],
  source: &'a str,
  index: usize,
}

impl<'a> Iterator for PotentialTokens<'a> {
  type Item = &'a str;

  fn next(&mut self) -> Option<Self::Item> {
    if let Some(&c) = self.bytes.get(self.index) {
      let start = self.index;
      let mut c = char::from(c);
      while c != '\n' && c != ';' && c != '{' && c != '}' {
        self.index += 1;
        if let Some(&ch) = self.bytes.get(self.index) {
          c = char::from(ch);
        } else {
          return Some(&self.source[start..self.index]);
        }
      }
      while c == ';'
        || c == ' '
        || c == '{'
        || c == '}'
        || c == '\r'
        || c == '\t'
      {
        self.index += 1;
        if let Some(&ch) = self.bytes.get(self.index) {
          c = char::from(ch);
        } else {
          return Some(&self.source[start..self.index]);
        }
      }
      if c == '\n' {
        self.index += 1;
      }
      Some(&self.source[start..self.index])
    } else {
      None
    }
  }
}

// /[^\n;{}]+[;{} \r\t]*\n?|[;{} \r\t]+\n?|\n/g
pub fn split_into_potential_tokens(source: &str) -> PotentialTokens {
  PotentialTokens {
    bytes: source.as_bytes(),
    source,
    index: 0,
  }
}

/// Split the string with a needle, each string will contain the needle.
///
/// Copied and modified from https://github.com/rust-lang/cargo/blob/30efe860c0e4adc1a6d7057ad223dc6e47d34edf/src/cargo/sources/registry/index.rs#L1048-L1072
fn split(haystack: &str, needle: u8) -> impl Iterator<Item = &str> {
  struct Split<'a> {
    haystack: &'a str,
    needle: u8,
  }

  impl<'a> Iterator for Split<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
      if self.haystack.is_empty() {
        return None;
      }
      let (ret, remaining) =
        match memchr::memchr(self.needle, self.haystack.as_bytes()) {
          Some(pos) => (&self.haystack[..=pos], &self.haystack[pos + 1..]),
          None => (self.haystack, ""),
        };
      self.haystack = remaining;
      Some(ret)
    }
  }

  Split { haystack, needle }
}

// /[^\n]+\n?|\n/g
pub fn split_into_lines(source: &str) -> impl Iterator<Item = &str> {
  split(source, b'\n')
}

pub fn get_generated_source_info(source: &str) -> GeneratedInfo {
  let (generated_line, generated_column) = if source.ends_with('\n') {
    (split(source, b'\n').count() + 1, 0)
  } else {
    let mut line_count = 0;
    let mut last_line = "";
    for line in split(source, b'\n') {
      line_count += 1;
      last_line = line;
    }
    (line_count.max(1), last_line.len())
  };
  GeneratedInfo {
    generated_line: generated_line as u32,
    generated_column: generated_column as u32,
  }
}

pub fn stream_chunks_of_raw_source(
  source: &str,
  options: &MapOptions,
  on_chunk: OnChunk,
  _on_source: OnSource,
  _on_name: OnName,
) -> GeneratedInfo {
  if options.final_source {
    return get_generated_source_info(source);
  }

  let mut line = 1;
  let mut last_line = None;
  for l in split_into_lines(source) {
    on_chunk(
      Some(l),
      Mapping {
        generated_line: line,
        generated_column: 0,
        original: None,
      },
    );
    line += 1;
    last_line = Some(l);
  }
  if let Some(last_line) =
    last_line.filter(|last_line| !last_line.ends_with('\n'))
  {
    GeneratedInfo {
      generated_line: line - 1,
      generated_column: last_line.len() as u32,
    }
  } else {
    GeneratedInfo {
      generated_line: line,
      generated_column: 0,
    }
  }
}

pub fn stream_chunks_of_source_map(
  source: &str,
  source_map: &SourceMap,
  on_chunk: OnChunk,
  on_source: OnSource,
  on_name: OnName,
  options: &MapOptions,
) -> GeneratedInfo {
  match options {
    MapOptions {
      columns: true,
      final_source: true,
    } => stream_chunks_of_source_map_final(
      source, source_map, on_chunk, on_source, on_name,
    ),
    MapOptions {
      columns: true,
      final_source: false,
    } => stream_chunks_of_source_map_full(
      source, source_map, on_chunk, on_source, on_name,
    ),
    MapOptions {
      columns: false,
      final_source: true,
    } => stream_chunks_of_source_map_lines_final(
      source, source_map, on_chunk, on_source, on_name,
    ),
    MapOptions {
      columns: false,
      final_source: false,
    } => stream_chunks_of_source_map_lines_full(
      source, source_map, on_chunk, on_source, on_name,
    ),
  }
}

fn get_source(source_map: &SourceMap, source: &str) -> String {
  let source_root = source_map.source_root();
  match source_root {
    Some(root) if root.is_empty() => source.to_string(),
    Some(root) if root.ends_with('/') => format!("{}{}", root, source),
    Some(root) => format!("{}/{}", root, source),
    None => source.to_string(),
  }
}

fn stream_chunks_of_source_map_final(
  source: &str,
  source_map: &SourceMap,
  on_chunk: OnChunk,
  on_source: OnSource,
  on_name: OnName,
) -> GeneratedInfo {
  let result = get_generated_source_info(source);
  if result.generated_line == 1 && result.generated_column == 0 {
    return result;
  }
  for (i, source) in source_map.sources().iter().enumerate() {
    on_source(
      i as u32,
      &get_source(source_map, source),
      source_map.get_source_content(i),
    )
  }
  for (i, name) in source_map.names().iter().enumerate() {
    on_name(i as u32, name);
  }
  let mut mapping_active_line = 0;
  let mut on_mapping = |mapping: &Mapping| {
    if mapping.generated_line >= result.generated_line
      && (mapping.generated_column >= result.generated_column
        || mapping.generated_line > result.generated_line)
    {
      return;
    }
    if let Some(original) = &mapping.original {
      on_chunk(
        None,
        Mapping {
          generated_line: mapping.generated_line,
          generated_column: mapping.generated_column,
          original: Some(*original),
        },
      );
      mapping_active_line = mapping.generated_line;
    } else if mapping_active_line == mapping.generated_line {
      on_chunk(
        None,
        Mapping {
          generated_line: mapping.generated_line,
          generated_column: mapping.generated_column,
          original: None,
        },
      );
    }
  };
  for mapping in source_map.decoded_mappings() {
    on_mapping(mapping);
  }
  result
}

fn stream_chunks_of_source_map_full(
  source: &str,
  source_map: &SourceMap,
  on_chunk: OnChunk,
  on_source: OnSource,
  on_name: OnName,
) -> GeneratedInfo {
  for (i, source) in source_map.sources().iter().enumerate() {
    on_source(
      i as u32,
      &get_source(source_map, source),
      source_map.get_source_content(i),
    )
  }

  for (i, name) in source_map.names().iter().enumerate() {
    on_name(i as u32, name);
  }

  let mut current_generated_line: u32 = 1;
  let mut current_generated_column: u32 = 0;

  let mut tracking_generated_index: usize = 0;
  let mut tracking_generated_line: u32 = 1;
  let mut tracking_generated_column: u32 = 0;
  let mut tracking_mapping_original: Option<OriginalLocation> = None;

  let mut mappings_iter = source_map.decoded_mappings().iter();
  let mut current_mapping = mappings_iter.next();

  for (current_generated_index, c) in source.char_indices() {
    if let Some(mapping) = current_mapping.take() {
      if mapping.generated_line == current_generated_line
        && mapping.generated_column == current_generated_column
      {
        let chunk = &source[tracking_generated_index..current_generated_index];
        if !chunk.is_empty() {
          on_chunk(
            Some(chunk),
            Mapping {
              generated_line: tracking_generated_line,
              generated_column: tracking_generated_column,
              original: tracking_mapping_original,
            },
          );
        }

        tracking_generated_index = current_generated_index;
        tracking_generated_line = mapping.generated_line;
        tracking_generated_column = mapping.generated_column;
        tracking_mapping_original = mapping.original;

        current_mapping = mappings_iter.next();
      } else {
        current_mapping = Some(mapping);
      }
    }

    current_generated_column += 1;
    if c == '\n' {
      if tracking_generated_line == current_generated_line {
        let chunk =
          &source[tracking_generated_index..current_generated_index + 1];
        on_chunk(
          Some(chunk),
          Mapping {
            generated_line: tracking_generated_line,
            generated_column: tracking_generated_column,
            original: tracking_mapping_original,
          },
        );

        tracking_generated_index = current_generated_index + 1;
        tracking_generated_line += 1;
        tracking_generated_column = 0;
        tracking_mapping_original = None;
      }

      current_generated_line += 1;
      current_generated_column = 0;
    }
  }

  if tracking_generated_index < source.len() {
    let chunk = &source[tracking_generated_index..];
    on_chunk(
      Some(chunk),
      Mapping {
        generated_line: tracking_generated_line,
        generated_column: tracking_generated_column,
        original: tracking_mapping_original,
      },
    );
  }

  GeneratedInfo {
    generated_line: current_generated_line,
    generated_column: current_generated_column,
  }
}

fn stream_chunks_of_source_map_lines_final(
  source: &str,
  source_map: &SourceMap,
  on_chunk: OnChunk,
  on_source: OnSource,
  _on_name: OnName,
) -> GeneratedInfo {
  let result = get_generated_source_info(source);
  if result.generated_line == 1 && result.generated_column == 0 {
    return GeneratedInfo {
      generated_line: 1,
      generated_column: 0,
    };
  }
  for (i, source) in source_map.sources().iter().enumerate() {
    on_source(
      i as u32,
      &get_source(source_map, source),
      source_map.get_source_content(i),
    )
  }
  let final_line = if result.generated_column == 0 {
    result.generated_line - 1
  } else {
    result.generated_line
  };
  let mut current_generated_line = 1;

  let mut on_mapping = |mapping: &Mapping| {
    if let Some(original) = &mapping.original.filter(|_| {
      current_generated_line <= mapping.generated_line
        && mapping.generated_line <= final_line
    }) {
      on_chunk(
        None,
        Mapping {
          generated_line: mapping.generated_line,
          generated_column: 0,
          original: Some(OriginalLocation {
            source_index: original.source_index,
            original_line: original.original_line,
            original_column: original.original_column,
            name_index: None,
          }),
        },
      );
      current_generated_line = mapping.generated_line + 1;
    }
  };
  for mapping in source_map.decoded_mappings() {
    on_mapping(mapping);
  }
  result
}

fn stream_chunks_of_source_map_lines_full(
  source: &str,
  source_map: &SourceMap,
  on_chunk: OnChunk,
  on_source: OnSource,
  _on_name: OnName,
) -> GeneratedInfo {
  let lines: Vec<&str> = split_into_lines(source).collect();
  if lines.is_empty() {
    return GeneratedInfo {
      generated_line: 1,
      generated_column: 0,
    };
  }
  for (i, source) in source_map.sources().iter().enumerate() {
    on_source(
      i as u32,
      &get_source(source_map, source),
      source_map.get_source_content(i),
    )
  }
  let mut current_generated_line = 1;
  let mut on_mapping = |mapping: &Mapping| {
    if mapping.original.is_none()
      || mapping.generated_line < current_generated_line
      || mapping.generated_line as usize > lines.len()
    {
      return;
    }
    while mapping.generated_line > current_generated_line {
      if current_generated_line as usize <= lines.len() {
        on_chunk(
          Some(lines[current_generated_line as usize - 1]),
          Mapping {
            generated_line: current_generated_line,
            generated_column: 0,
            original: None,
          },
        );
      }
      current_generated_line += 1;
    }
    if let Some(original) = &mapping
      .original
      .filter(|_| mapping.generated_line as usize <= lines.len())
    {
      on_chunk(
        Some(lines[mapping.generated_line as usize - 1]),
        Mapping {
          generated_line: mapping.generated_line,
          generated_column: 0,
          original: Some(OriginalLocation {
            source_index: original.source_index,
            original_line: original.original_line,
            original_column: original.original_column,
            name_index: None,
          }),
        },
      );
      current_generated_line += 1;
    }
  };
  for mapping in source_map.decoded_mappings() {
    on_mapping(mapping);
  }
  while current_generated_line as usize <= lines.len() {
    on_chunk(
      Some(lines[current_generated_line as usize - 1]),
      Mapping {
        generated_line: current_generated_line,
        generated_column: 0,
        original: None,
      },
    );
    current_generated_line += 1;
  }
  let last_line = lines[lines.len() - 1];
  let last_new_line = last_line.ends_with('\n');
  let final_line = if last_new_line {
    lines.len() + 1
  } else {
    lines.len()
  } as u32;
  let final_column = if last_new_line { 0 } else { last_line.len() } as u32;
  GeneratedInfo {
    generated_line: final_line,
    generated_column: final_column,
  }
}

#[derive(Debug)]
struct SourceMapLineData {
  pub mappings_data: Vec<i64>,
  pub chunks: Vec<SourceMapLineChunk>,
}

#[derive(Debug)]
struct SourceMapLineChunk {
  content: ArcStr,
  cached: OnceCell<WithIndices<ArcStr>>,
}

impl SourceMapLineChunk {
  pub fn new(content: ArcStr) -> Self {
    Self {
      content,
      cached: OnceCell::new(),
    }
  }

  pub fn substring(&self, start_index: usize, end_index: usize) -> &str {
    let cached = self
      .cached
      .get_or_init(|| WithIndices::new(self.content.clone()));
    cached.substring(start_index, end_index)
  }
}

#[allow(clippy::too_many_arguments)]
pub fn stream_chunks_of_combined_source_map(
  source: &str,
  source_map: &SourceMap,
  inner_source_name: &str,
  inner_source: Option<&str>,
  inner_source_map: &SourceMap,
  remove_inner_source: bool,
  on_chunk: OnChunk,
  on_source: OnSource,
  on_name: OnName,
  options: &MapOptions,
) -> GeneratedInfo {
  let on_source = RefCell::new(on_source);
  let inner_source: RefCell<Option<ArcStr>> =
    RefCell::new(inner_source.map(Into::into));
  let source_mapping: RefCell<HashMap<ArcStr, u32>> =
    RefCell::new(HashMap::default());
  let mut name_mapping: HashMap<ArcStr, u32> = HashMap::default();
  let source_index_mapping: RefCell<Vec<i64>> =
    RefCell::new(Vec::with_capacity(source_map.sources().len()));
  let name_index_mapping: RefCell<Vec<i64>> =
    RefCell::new(Vec::with_capacity(source_map.names().len()));
  let name_index_value_mapping: RefCell<HashMap<i64, ArcStr>> =
    RefCell::new(HashMap::default());
  let inner_source_index: RefCell<i64> = RefCell::new(-2);
  let inner_source_index_mapping: RefCell<HashMap<i64, i64>> =
    RefCell::new(HashMap::default());
  let inner_source_index_value_mapping: RefCell<
    HashMap<i64, (ArcStr, Option<ArcStr>)>,
  > = RefCell::new(HashMap::default());
  let inner_source_contents: RefCell<HashMap<i64, Option<ArcStr>>> =
    RefCell::new(HashMap::default());
  let inner_source_content_lines: InnerSourceContentLine =
    RefCell::new(HashMap::default());
  let inner_name_index_mapping: RefCell<HashMap<i64, i64>> =
    RefCell::new(HashMap::default());
  let inner_name_index_value_mapping: RefCell<HashMap<i64, ArcStr>> =
    RefCell::new(HashMap::default());
  let inner_source_map_line_data: RefCell<Vec<SourceMapLineData>> =
    RefCell::new(Vec::new());
  let find_inner_mapping = |line: i64, column: i64| -> Option<u32> {
    let inner_source_map_line_data = inner_source_map_line_data.borrow();
    if line as usize > inner_source_map_line_data.len() {
      return None;
    }
    let mappings_data =
      &inner_source_map_line_data[line as usize - 1].mappings_data;
    let mut l = 0;
    let mut r = mappings_data.len() / 5;
    while l < r {
      let m = (l + r) >> 1;
      if mappings_data[m * 5] <= column {
        l = m + 1;
      } else {
        r = m;
      }
    }
    if l == 0 {
      return None;
    }
    Some(l as u32 - 1)
  };
  stream_chunks_of_source_map(
    source,
    source_map,
    &mut |chunk, mapping| {
      let source_index = mapping
        .original
        .as_ref()
        .map_or(-1, |o| o.source_index as i64);
      let original_line = mapping
        .original
        .as_ref()
        .map_or(-1, |o| o.original_line as i64);
      let original_column = mapping
        .original
        .as_ref()
        .map_or(-1, |o| o.original_column as i64);
      let name_index = mapping
        .original
        .as_ref()
        .and_then(|o| o.name_index)
        .map(|i| i as i64)
        .unwrap_or(-1);

      // Check if this is a mapping to the inner source
      if source_index == *inner_source_index.borrow() {
        // Check if there is a mapping in the inner source
        if let Some(idx) = find_inner_mapping(original_line, original_column) {
          let idx = idx as usize;
          let SourceMapLineData {
            mappings_data,
            chunks,
          } = &inner_source_map_line_data.borrow()[original_line as usize - 1];
          let mi = idx * 5;
          let inner_source_index = mappings_data[mi + 1];
          let inner_original_line = mappings_data[mi + 2];
          let mut inner_original_column = mappings_data[mi + 3];
          let mut inner_name_index = mappings_data[mi + 4];
          if inner_source_index >= 0 {
            // Check for an identity mapping
            // where we are allowed to adjust the original column
            let inner_chunk = &chunks[idx];
            let inner_generated_column = mappings_data[mi];
            let location_in_chunk = original_column - inner_generated_column;
            if location_in_chunk > 0 {
              let mut inner_source_content_lines =
                inner_source_content_lines.borrow_mut();
              let mut original_source_lines = inner_source_content_lines
                .get(&inner_source_index)
                .cloned()
                .and_then(|id| id);
              if original_source_lines.is_none() {
                let inner_source_contents = inner_source_contents.borrow();
                original_source_lines = if let Some(Some(original_source)) =
                  inner_source_contents.get(&inner_source_index)
                {
                  Some(Rc::new(
                    split_into_lines(original_source)
                      .map(|s| WithIndices::new(s.into()))
                      .collect(),
                  ))
                } else {
                  None
                };
                inner_source_content_lines
                  .insert(inner_source_index, original_source_lines.clone());
              }
              if let Some(original_source_lines) = original_source_lines {
                let original_chunk = original_source_lines
                  .get(inner_original_line as usize - 1)
                  .map_or("", |lines| {
                    let start = inner_original_column as usize;
                    let end = start + location_in_chunk as usize;
                    lines.substring(start, end)
                  });
                if inner_chunk.substring(0, location_in_chunk as usize)
                  == original_chunk
                {
                  inner_original_column += location_in_chunk;
                  inner_name_index = -1;
                }
              }
            }

            // We have a inner mapping to original source

            // emit source when needed and compute global source index
            let mut inner_source_index_mapping =
              inner_source_index_mapping.borrow_mut();
            let mut source_index = inner_source_index_mapping
              .get(&inner_source_index)
              .copied()
              .unwrap_or(-2);
            if source_index == -2 {
              let (source, source_content) = inner_source_index_value_mapping
                .borrow()
                .get(&inner_source_index)
                .cloned()
                .unwrap_or(("".into(), None));
              let mut source_mapping = source_mapping.borrow_mut();
              let mut global_index = source_mapping.get(&source).copied();
              if global_index.is_none() {
                let len = source_mapping.len() as u32;
                source_mapping.insert(source.clone(), len);
                on_source.borrow_mut()(len, &source, source_content.as_deref());
                global_index = Some(len);
              }
              source_index = global_index.unwrap() as i64;
              inner_source_index_mapping
                .insert(inner_source_index, source_index);
            }

            // emit name when needed and compute global name index
            let mut final_name_index = -1;
            if inner_name_index >= 0 {
              // when we have a inner name
              let mut inner_name_index_mapping =
                inner_name_index_mapping.borrow_mut();
              final_name_index = inner_name_index_mapping
                .get(&inner_name_index)
                .copied()
                .unwrap_or(-2);
              if final_name_index == -2 {
                if let Some(name) = inner_name_index_value_mapping
                  .borrow()
                  .get(&inner_name_index)
                {
                  let mut global_index = name_mapping.get(name).copied();
                  if global_index.is_none() {
                    let len = name_mapping.len() as u32;
                    name_mapping.insert(name.to_owned(), len);
                    on_name(len, name);
                    global_index = Some(len);
                  }
                  final_name_index = global_index.unwrap() as i64;
                } else {
                  final_name_index = -1;
                }
                inner_name_index_mapping
                  .insert(inner_name_index, final_name_index);
              }
            } else if name_index >= 0 {
              // when we don't have an inner name,
              // but we have an outer name
              // it can be used when inner original code equals to the name
              let mut inner_source_content_lines =
                inner_source_content_lines.borrow_mut();
              let mut original_source_lines = inner_source_content_lines
                .get(&inner_source_index)
                .cloned()
                .and_then(|id| id);
              if original_source_lines.is_none() {
                let inner_source_contents = inner_source_contents.borrow_mut();
                original_source_lines = inner_source_contents
                  .get(&inner_source_index)
                  .and_then(|original_source| {
                    original_source.as_ref().map(|s| {
                      let lines = split_into_lines(s);
                      Rc::new(
                        lines
                          .into_iter()
                          .map(|s| WithIndices::new(s.into()))
                          .collect::<Vec<_>>(),
                      )
                    })
                  });
                inner_source_content_lines
                  .insert(inner_source_index, original_source_lines.clone());
              }
              if let Some(original_source_lines) = original_source_lines {
                let name_index_value_mapping =
                  name_index_value_mapping.borrow();
                let name =
                  name_index_value_mapping.get(&name_index).cloned().unwrap();
                let original_name = original_source_lines
                  .get(inner_original_line as usize - 1)
                  .map_or("", |i| {
                    let start = inner_original_column as usize;
                    let end = start + name.len();
                    i.substring(start, end)
                  });
                if name.as_ref() == original_name {
                  let mut name_index_mapping = name_index_mapping.borrow_mut();
                  final_name_index = name_index_mapping
                    .get(name_index as usize)
                    .copied()
                    .unwrap_or(-2);
                  if final_name_index == -2 {
                    if let Some(name) =
                      name_index_value_mapping.get(&name_index)
                    {
                      let mut global_index = name_mapping.get(name).copied();
                      if global_index.is_none() {
                        let len = name_mapping.len() as u32;
                        name_mapping.insert(name.to_owned(), len);
                        on_name(len, name);
                        global_index = Some(len);
                      }
                      final_name_index = global_index.unwrap() as i64;
                    } else {
                      final_name_index = -1;
                    }
                    name_index_mapping
                      .insert(name_index as usize, final_name_index);
                  }
                }
              }
            }
            on_chunk(
              chunk,
              Mapping {
                generated_line: mapping.generated_line,
                generated_column: mapping.generated_column,
                original: (source_index >= 0).then_some(OriginalLocation {
                  source_index: source_index as u32,
                  original_line: inner_original_line as u32,
                  original_column: inner_original_column as u32,
                  name_index: (final_name_index >= 0)
                    .then_some(final_name_index as u32),
                }),
              },
            );
            return;
          }
        }

        // We have a mapping to the inner source, but no inner mapping
        if remove_inner_source {
          on_chunk(
            chunk,
            Mapping {
              generated_line: mapping.generated_line,
              generated_column: mapping.generated_column,
              original: None,
            },
          );
          return;
        } else {
          let mut source_index_mapping = source_index_mapping.borrow_mut();
          if source_index_mapping.get(source_index as usize) == Some(&-2) {
            let mut source_mapping = source_mapping.borrow_mut();
            let mut global_index =
              source_mapping.get(inner_source_name).copied();
            if global_index.is_none() {
              let len = source_mapping.len() as u32;
              source_mapping.insert(source.into(), len);
              on_source.borrow_mut()(
                len,
                inner_source_name,
                inner_source.borrow().as_deref(),
              );
              global_index = Some(len);
            }
            source_index_mapping
              .insert(source_index as usize, global_index.unwrap() as i64);
          }
        }
      }

      let source_index_mapping = source_index_mapping.borrow();
      let final_source_index = source_index_mapping
        .get(source_index as usize)
        .copied()
        .unwrap_or(-1);
      if final_source_index < 0 {
        // no source, so we make it a generated chunk
        on_chunk(
          chunk,
          Mapping {
            generated_line: mapping.generated_line,
            generated_column: mapping.generated_column,
            original: None,
          },
        );
      } else {
        // Pass through the chunk with mapping
        let mut name_index_mapping = name_index_mapping.borrow_mut();
        let mut final_name_index = name_index_mapping
          .get(name_index as usize)
          .copied()
          .unwrap_or(-1);
        if final_name_index == -2 {
          let name_index_value_mapping = name_index_value_mapping.borrow();
          let name = name_index_value_mapping.get(&name_index).unwrap();
          let mut global_index = name_mapping.get(name).copied();
          if global_index.is_none() {
            let len = name_mapping.len() as u32;
            name_mapping.borrow_mut().insert(name.to_owned(), len);
            on_name(len, name);
            global_index = Some(len);
          }
          final_name_index = global_index.unwrap() as i64;
          name_index_mapping.insert(name_index as usize, final_name_index);
        }
        on_chunk(
          chunk,
          Mapping {
            generated_line: mapping.generated_line,
            generated_column: mapping.generated_column,
            original: (final_source_index >= 0).then_some(OriginalLocation {
              source_index: final_source_index as u32,
              original_line: original_line as u32,
              original_column: original_column as u32,
              name_index: (final_name_index >= 0)
                .then_some(final_name_index as u32),
            }),
          },
        );
      }
    },
    &mut |i, source, source_content| {
      let i = i as i64;
      let mut source_content: Option<ArcStr> = source_content.map(Into::into);
      if source == inner_source_name {
        *inner_source_index.borrow_mut() = i;
        let mut inner_source = inner_source.borrow_mut();
        if let Some(inner_source) = inner_source.as_ref() {
          source_content = Some(inner_source.clone());
        } else {
          *inner_source = source_content.clone();
        }
        source_index_mapping.borrow_mut().insert(i as usize, -2);
        stream_chunks_of_source_map(
          &source_content.unwrap(),
          inner_source_map,
          &mut |chunk, mapping| {
            let mut inner_source_map_line_data =
              inner_source_map_line_data.borrow_mut();
            let inner_source_map_line_data_len =
              inner_source_map_line_data.len();
            let mapping_generated_line_len = mapping.generated_line as usize;
            if inner_source_map_line_data_len <= mapping_generated_line_len {
              inner_source_map_line_data.reserve(
                mapping_generated_line_len - inner_source_map_line_data_len + 1,
              );
              while inner_source_map_line_data.len()
                <= mapping_generated_line_len
              {
                inner_source_map_line_data.push(SourceMapLineData {
                  mappings_data: Default::default(),
                  chunks: vec![],
                });
              }
            }
            let data = &mut inner_source_map_line_data
              [mapping.generated_line as usize - 1];
            data.mappings_data.reserve(5);
            data.mappings_data.push(mapping.generated_column as i64);
            data.mappings_data.push(
              mapping
                .original
                .as_ref()
                .map_or(-1, |original| original.source_index as i64),
            );
            data.mappings_data.push(
              mapping
                .original
                .as_ref()
                .map_or(-1, |original| original.original_line as i64),
            );
            data.mappings_data.push(
              mapping
                .original
                .as_ref()
                .map_or(-1, |original| original.original_column as i64),
            );
            data.mappings_data.push(
              mapping
                .original
                .and_then(|original| original.name_index)
                .map(Into::into)
                .unwrap_or(-1),
            );
            // SAFETY: final_source is false
            let chunk = SourceMapLineChunk::new(chunk.unwrap().into());
            data.chunks.push(chunk);
          },
          &mut |i, source, source_content| {
            let i = i as i64;
            inner_source_contents
              .borrow_mut()
              .insert(i, source_content.map(Into::into));
            inner_source_content_lines.borrow_mut().insert(i, None);
            inner_source_index_mapping.borrow_mut().insert(i, -2);
            inner_source_index_value_mapping
              .borrow_mut()
              .insert(i, (source.into(), source_content.map(Into::into)));
          },
          &mut |i, name| {
            let i = i as i64;
            inner_name_index_mapping.borrow_mut().insert(i, -2);
            inner_name_index_value_mapping
              .borrow_mut()
              .insert(i, name.into());
          },
          &MapOptions {
            columns: options.columns,
            final_source: false,
          },
        );
      } else {
        let mut source_mapping = source_mapping.borrow_mut();
        let mut global_index = source_mapping.get(source).copied();
        if global_index.is_none() {
          let len = source_mapping.len() as u32;
          source_mapping.insert(source.into(), len);
          on_source.borrow_mut()(len, source, source_content.as_deref());
          global_index = Some(len);
        }
        source_index_mapping
          .borrow_mut()
          .insert(i as usize, global_index.unwrap() as i64);
      }
    },
    &mut |i, name| {
      let i = i as i64;
      name_index_mapping.borrow_mut().insert(i as usize, -2);
      name_index_value_mapping.borrow_mut().insert(i, name.into());
    },
    options,
  )
}

pub fn stream_and_get_source_and_map<S: StreamChunks>(
  input_source: &S,
  options: &MapOptions,
  on_chunk: OnChunk,
  on_source: OnSource,
  on_name: OnName,
) -> (GeneratedInfo, Option<SourceMap>) {
  let mut mappings = Vec::new();
  let mut sources: Vec<Cow<'static, str>> = Vec::new();
  let mut sources_content: Vec<Cow<'static, str>> = Vec::new();
  let mut names: Vec<Cow<'static, str>> = Vec::new();
  let generated_info = input_source.stream_chunks(
    options,
    &mut |chunk, mapping| {
      mappings.push(mapping.clone());
      on_chunk(chunk, mapping);
    },
    &mut |source_index, source, source_content| {
      let source_index2 = source_index as usize;
      while sources.len() <= source_index2 {
        sources.push("".into());
      }
      sources[source_index2] = source.to_string().into();
      if let Some(source_content) = source_content {
        while sources_content.len() <= source_index2 {
          sources_content.push("".into());
        }
        sources_content[source_index2] = source_content.to_string().into();
      }
      on_source(source_index, source, source_content);
    },
    &mut |name_index, name| {
      let name_index2 = name_index as usize;
      while names.len() <= name_index2 {
        names.push("".into());
      }
      names[name_index2] = name.to_string().into();
      on_name(name_index, name);
    },
  );

  let mappings = encode_mappings(&mappings, options);
  let map = if mappings.is_empty() {
    None
  } else {
    Some(SourceMap::new(
      None,
      mappings,
      sources,
      sources_content,
      names,
    ))
  };
  (generated_info, map)
}
