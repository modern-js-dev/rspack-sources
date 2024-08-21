use std::{
  borrow::Cow,
  cell::RefCell,
  collections::hash_map::Entry,
  hash::{Hash, Hasher},
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, MutexGuard, OnceLock,
  },
};

use rustc_hash::FxHashMap as HashMap;

use crate::{
  helpers::{get_map, split_into_lines, GeneratedInfo, StreamChunks},
  with_indices::WithIndices,
  MapOptions, Mapping, OriginalLocation, Source, SourceMap,
};

/// Decorates a Source with replacements and insertions of source code,
/// usually used in dependencies
///
/// - [webpack-sources docs](https://github.com/webpack/webpack-sources/#replacesource).
///
/// ```
/// use rspack_sources::{OriginalSource, ReplaceSource, Source};
///
/// let code = "hello world\n";
/// let mut source = ReplaceSource::new(OriginalSource::new(code, "file.txt"));
///
/// source.insert(0, "start1\n", None);
/// source.replace(0, 0, "start2\n", None);
/// source.replace(999, 10000, "end2", None);
/// source.insert(888, "end1\n", None);
/// source.replace(0, 999, "replaced!\n", Some("whole"));
///
/// assert_eq!(source.source(), "start1\nstart2\nreplaced!\nend1\nend2");
/// ```
pub struct ReplaceSource<T> {
  inner: Arc<T>,
  inner_source_code: OnceLock<Box<str>>,
  replacements: Mutex<Vec<Replacement>>,
  /// Whether `replacements` is sorted.
  is_sorted: AtomicBool,
}

/// Enforce replacement order when two replacement start and end are both equal
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReplacementEnforce {
  /// pre
  Pre,
  /// normal
  #[default]
  Normal,
  /// post
  Post,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Replacement {
  start: u32,
  end: u32,
  content: String,
  name: Option<String>,
  enforce: ReplacementEnforce,
}

impl Replacement {
  pub fn new(
    start: u32,
    end: u32,
    content: String,
    name: Option<String>,
    enforce: ReplacementEnforce,
  ) -> Self {
    Self {
      start,
      end,
      content,
      name,
      enforce,
    }
  }
}

impl<T> ReplaceSource<T> {
  /// Create a [ReplaceSource].
  pub fn new(source: T) -> Self {
    Self {
      inner: Arc::new(source),
      inner_source_code: OnceLock::new(),
      replacements: Mutex::new(Vec::new()),
      is_sorted: AtomicBool::new(true),
    }
  }

  /// Get the original [Source].
  pub fn original(&self) -> &T {
    &self.inner
  }

  fn replacements(&self) -> MutexGuard<Vec<Replacement>> {
    self.replacements.lock().unwrap()
  }

  fn sort_replacement(&self) {
    if self.is_sorted.load(Ordering::SeqCst) {
      return;
    }
    self.replacements().sort_by(|a, b| {
      (a.start, a.end, a.enforce).cmp(&(b.start, b.end, b.enforce))
    });
    self.is_sorted.store(true, Ordering::SeqCst)
  }
}

impl<T: Source> ReplaceSource<T> {
  fn get_inner_source_code(&self) -> &str {
    self
      .inner_source_code
      .get_or_init(|| Box::from(self.inner.source()))
  }

  /// Insert a content at start.
  pub fn insert(&mut self, start: u32, content: &str, name: Option<&str>) {
    self.replace(start, start, content, name)
  }

  /// Insert a content at start, with ReplacementEnforce.
  pub fn insert_with_enforce(
    &mut self,
    start: u32,
    content: &str,
    name: Option<&str>,
    enforce: ReplacementEnforce,
  ) {
    self.replace_with_enforce(start, start, content, name, enforce)
  }

  /// Create a replacement with content at `[start, end)`.
  pub fn replace(
    &mut self,
    start: u32,
    end: u32,
    content: &str,
    name: Option<&str>,
  ) {
    self.replacements().push(Replacement::new(
      start,
      end,
      content.into(),
      name.map(|s| s.into()),
      ReplacementEnforce::Normal,
    ));
    self.is_sorted.store(false, Ordering::SeqCst);
  }

  /// Create a replacement with content at `[start, end)`, with ReplacementEnforce.
  pub fn replace_with_enforce(
    &mut self,
    start: u32,
    end: u32,
    content: &str,
    name: Option<&str>,
    enforce: ReplacementEnforce,
  ) {
    self.replacements().push(Replacement::new(
      start,
      end,
      content.into(),
      name.map(|s| s.into()),
      enforce,
    ));
    self.is_sorted.store(false, Ordering::SeqCst);
  }
}

impl<T: Source + Hash + PartialEq + Eq + 'static> Source for ReplaceSource<T> {
  fn source(&self) -> Cow<str> {
    self.sort_replacement();

    let inner_source_code = self.get_inner_source_code();

    // mut_string_push_str is faster that vec join
    // concatenate strings benchmark, see https://github.com/hoodie/concatenation_benchmarks-rs
    let mut source_code = String::new();
    let mut inner_pos = 0;
    for replacement in self.replacements.lock().unwrap().iter() {
      if inner_pos < replacement.start {
        let end_pos = (replacement.start as usize).min(inner_source_code.len());
        source_code.push_str(&inner_source_code[inner_pos as usize..end_pos]);
      }
      source_code.push_str(&replacement.content);
      #[allow(clippy::manual_clamp)]
      {
        inner_pos = inner_pos
          .max(replacement.end)
          .min(inner_source_code.len() as u32);
      }
    }
    source_code.push_str(
      &inner_source_code[inner_pos as usize..inner_source_code.len()],
    );

    source_code.into()
  }

  fn buffer(&self) -> Cow<[u8]> {
    let source = self.source().to_string();
    Cow::Owned(source.into_bytes())
  }

  fn size(&self) -> usize {
    self.source().len()
  }

  fn map(&self, options: &crate::MapOptions) -> Option<SourceMap> {
    let replacements = self.replacements.lock().unwrap();
    if replacements.is_empty() {
      return self.inner.map(options);
    }
    drop(replacements);
    get_map(self, options)
  }

  fn to_writer(&self, writer: &mut dyn std::io::Write) -> std::io::Result<()> {
    writer.write_all(self.source().as_bytes())
  }
}

impl<T: std::fmt::Debug> std::fmt::Debug for ReplaceSource<T> {
  fn fmt(
    &self,
    f: &mut std::fmt::Formatter<'_>,
  ) -> Result<(), std::fmt::Error> {
    f.debug_struct("ReplaceSource")
      .field("inner", self.inner.as_ref())
      .field(
        "inner_source_code",
        &self
          .inner_source_code
          .get()
          .map(|s| s.chars().take(50).collect::<String>()),
      )
      .field(
        "replacements",
        &self.replacements.lock().iter().take(3).collect::<Vec<_>>(),
      )
      .field("is_sorted", &self.is_sorted.load(Ordering::SeqCst))
      .finish()
  }
}

impl<T: Source> StreamChunks for ReplaceSource<T> {
  fn stream_chunks(
    &self,
    options: &crate::MapOptions,
    on_chunk: crate::helpers::OnChunk,
    on_source: crate::helpers::OnSource,
    on_name: crate::helpers::OnName,
  ) -> crate::helpers::GeneratedInfo {
    self.sort_replacement();
    let on_name = RefCell::new(on_name);
    let repls = self.replacements();
    let mut pos: u32 = 0;
    let mut i: usize = 0;
    let mut replacement_end: Option<u32> = None;
    let mut next_replacement = (i < repls.len()).then(|| repls[i].start);
    let mut generated_line_offset: i64 = 0;
    let mut generated_column_offset: i64 = 0;
    let mut generated_column_offset_line = 0;
    let source_content_lines: RefCell<Vec<Option<Vec<String>>>> =
      RefCell::new(Vec::new());
    let name_mapping: RefCell<HashMap<String, u32>> =
      RefCell::new(HashMap::default());
    let name_index_mapping: RefCell<Vec<u32>> = RefCell::new(Vec::new());

    // check if source_content[line][col] is equal to expect
    // Why this is needed?
    //
    // For example, there is an source_map like (It's OriginalSource)
    //    source_code: "jsx || tsx"
    //    mappings:    ↑
    //    target_code: "jsx || tsx"
    // If replace || to &&, there will be some new mapping information
    //    source_code: "jsx || tsx"
    //    mappings:    ↑    ↑  ↑
    //    target_code: "jsx && tsx"
    //
    // In this case, because source_content[line][col] is equal to target, we can split this mapping correctly,
    // Therefore, we can add some extra mappings for this replace operation.
    //
    // But for this example, source_content[line][col] is not equal to target (It's SourceMapSource)
    //    source_code: "<div />"
    //    mappings:    ↑
    //    target_code: "jsx || tsx"
    // If replace || to && also, then
    //    source_code: "<div />"
    //    mappings:    ↑
    //    target_code: "jsx && tsx"
    //
    // In this case, we can't split this mapping.
    // webpack-sources also have this function, refer https://github.com/webpack/webpack-sources/blob/main/lib/ReplaceSource.js#L158
    let check_original_content =
      |source_index: u32, line: u32, column: u32, expected_chunk: &str| {
        if let Some(Some(content_lines)) =
          source_content_lines.borrow().get(source_index as usize)
        {
          if let Some(content_line) = content_lines.get(line as usize - 1) {
            WithIndices::new(content_line).substring(
              column as usize,
              column as usize + expected_chunk.len(),
            ) == expected_chunk
          } else {
            false
          }
        } else {
          false
        }
      };
    let result = self.inner.stream_chunks(
      &MapOptions {
        columns: options.columns,
        final_source: false,
      },
      &mut |chunk, mut mapping| {
        // SAFETY: final_source is false in ReplaceSource
        let chunk = chunk.unwrap();
        let chunk_with_indices = WithIndices::new(chunk);
        let mut chunk_pos = 0;
        let end_pos = pos + chunk.len() as u32;
        // Skip over when it has been replaced
        if let Some(replacement_end) =
          replacement_end.filter(|replacement_end| *replacement_end > pos)
        {
          // Skip over the whole chunk
          if replacement_end >= end_pos {
            let line = mapping.generated_line as i64 + generated_line_offset;
            if chunk.ends_with('\n') {
              generated_line_offset -= 1;
              if generated_column_offset_line == line {
                // undo exiting corrections form the current line
                generated_column_offset += mapping.generated_column as i64;
              }
            } else if generated_column_offset_line == line {
              generated_column_offset -= chunk.len() as i64;
            } else {
              generated_column_offset = -(chunk.len() as i64);
              generated_column_offset_line = line;
            }
            pos = end_pos;
            return;
          }
          // Partially skip over chunk
          chunk_pos = replacement_end - pos;
          if let Some(original) = mapping.original.as_mut().filter(|original| {
            check_original_content(
              original.source_index,
              original.original_line,
              original.original_column,
              chunk_with_indices.substring(0, chunk_pos as usize),
            )
          }) {
            original.original_column += chunk_pos;
          }
          pos += chunk_pos;
          let line = mapping.generated_line as i64 + generated_line_offset;
          if generated_column_offset_line == line {
            generated_column_offset -= chunk_pos as i64;
          } else {
            generated_column_offset = -(chunk_pos as i64);
            generated_column_offset_line = line;
          }
          mapping.generated_column += chunk_pos;
        }

        // Is a replacement in the chunk?
        while let Some(next_replacement_pos) = next_replacement
          .filter(|next_replacement_pos| *next_replacement_pos < end_pos)
        {
          let mut line = mapping.generated_line as i64 + generated_line_offset;
          if next_replacement_pos > pos {
            // Emit chunk until replacement
            let offset = next_replacement_pos - pos;
            let chunk_slice = chunk_with_indices
              .substring(chunk_pos as usize, (chunk_pos + offset) as usize);
            on_chunk(
              Some(chunk_slice),
              Mapping {
                generated_line: line as u32,
                generated_column: ((mapping.generated_column as i64)
                  + if line == generated_column_offset_line {
                    generated_column_offset
                  } else {
                    0
                  }) as u32,
                original: mapping.original.as_ref().map(|original| {
                  OriginalLocation {
                    source_index: original.source_index,
                    original_line: original.original_line,
                    original_column: original.original_column,
                    name_index: original.name_index.and_then(|name_index| {
                      name_index_mapping
                        .borrow()
                        .get(name_index as usize)
                        .copied()
                    }),
                  }
                }),
              },
            );
            mapping.generated_column += offset;
            chunk_pos += offset;
            pos = next_replacement_pos;
            if let Some(original) =
              mapping.original.as_mut().filter(|original| {
                check_original_content(
                  original.source_index,
                  original.original_line,
                  original.original_column,
                  chunk_slice,
                )
              })
            {
              original.original_column += chunk_slice.len() as u32;
            }
          }
          // Insert replacement content split into chunks by lines
          let repl = &repls[i];
          let lines: Vec<&str> = split_into_lines(&repl.content).collect();
          let mut replacement_name_index = mapping
            .original
            .as_ref()
            .and_then(|original| original.name_index);
          if let Some(name) =
            repl.name.as_ref().filter(|_| mapping.original.is_some())
          {
            let mut name_mapping = name_mapping.borrow_mut();
            let len = name_mapping.len() as u32;
            let global_index = match name_mapping.entry(name.to_string()) {
              Entry::Occupied(entry) => *entry.get(),
              Entry::Vacant(entry) => {
                entry.insert(len);
                on_name.borrow_mut()(len, name);
                len
              }
            };
            replacement_name_index = Some(global_index);
          }
          for (m, content_line) in lines.iter().enumerate() {
            on_chunk(
              Some(content_line),
              Mapping {
                generated_line: line as u32,
                generated_column: ((mapping.generated_column as i64)
                  + if line == generated_column_offset_line {
                    generated_column_offset
                  } else {
                    0
                  }) as u32,
                original: mapping.original.as_ref().map(|original| {
                  OriginalLocation {
                    source_index: original.source_index,
                    original_line: original.original_line,
                    original_column: original.original_column,
                    name_index: replacement_name_index,
                  }
                }),
              },
            );
            // Only the first chunk has name assigned
            replacement_name_index = None;

            if m == lines.len() - 1 && !content_line.ends_with('\n') {
              if generated_column_offset_line == line {
                generated_column_offset += content_line.len() as i64;
              } else {
                generated_column_offset = content_line.len() as i64;
                generated_column_offset_line = line;
              }
            } else {
              generated_line_offset += 1;
              line += 1;
              generated_column_offset = -(mapping.generated_column as i64);
              generated_column_offset_line = line;
            }
          }

          // Remove replaced content by settings this variable
          replacement_end = if let Some(replacement_end) = replacement_end {
            Some(replacement_end.max(repl.end))
          } else {
            Some(repl.end)
          };

          // Move to next replacement
          i += 1;
          next_replacement = if i < repls.len() {
            Some(repls[i].start)
          } else {
            None
          };

          // Skip over when it has been replaced
          let offset = chunk.len() as i64 - end_pos as i64
            + replacement_end.unwrap() as i64
            - chunk_pos as i64;
          if offset > 0 {
            // Skip over whole chunk
            if replacement_end
              .is_some_and(|replacement_end| replacement_end >= end_pos)
            {
              let line = mapping.generated_line as i64 + generated_line_offset;
              if chunk.ends_with('\n') {
                generated_line_offset -= 1;
                if generated_column_offset_line == line {
                  // undo exiting corrections form the current line
                  generated_column_offset += mapping.generated_column as i64;
                }
              } else if generated_column_offset_line == line {
                generated_column_offset -=
                  chunk.len() as i64 - chunk_pos as i64;
              } else {
                generated_column_offset = chunk_pos as i64 - chunk.len() as i64;
                generated_column_offset_line = line;
              }
              pos = end_pos;
              return;
            }

            // Partially skip over chunk
            let line = mapping.generated_line as i64 + generated_line_offset;
            if let Some(original) =
              mapping.original.as_mut().filter(|original| {
                check_original_content(
                  original.source_index,
                  original.original_line,
                  original.original_column,
                  chunk_with_indices.substring(
                    chunk_pos as usize,
                    (chunk_pos + offset as u32) as usize,
                  ),
                )
              })
            {
              original.original_column += offset as u32;
            }
            chunk_pos += offset as u32;
            pos += offset as u32;
            if generated_column_offset_line == line {
              generated_column_offset -= offset;
            } else {
              generated_column_offset = -offset;
              generated_column_offset_line = line;
            }
            mapping.generated_column += offset as u32;
          }
        }

        // Emit remaining chunk
        if (chunk_pos as usize) < chunk.len() {
          let chunk_slice = if chunk_pos == 0 {
            chunk
          } else {
            chunk_with_indices.substring(chunk_pos as usize, usize::MAX)
          };
          let line = mapping.generated_line as i64 + generated_line_offset;
          on_chunk(
            Some(chunk_slice),
            Mapping {
              generated_line: line as u32,
              generated_column: ((mapping.generated_column as i64)
                + if line == generated_column_offset_line {
                  generated_column_offset
                } else {
                  0
                }) as u32,
              original: mapping.original.as_ref().map(|original| {
                OriginalLocation {
                  source_index: original.source_index,
                  original_line: original.original_line,
                  original_column: original.original_column,
                  name_index: original.name_index.and_then(|name_index| {
                    name_index_mapping
                      .borrow()
                      .get(name_index as usize)
                      .copied()
                  }),
                }
              }),
            },
          );
        }
        pos = end_pos;
      },
      &mut |source_index, source, source_content| {
        let mut source_content_lines = source_content_lines.borrow_mut();
        while source_content_lines.len() <= source_index as usize {
          source_content_lines.push(None);
        }
        source_content_lines[source_index as usize] =
          source_content.map(|source_content| {
            split_into_lines(source_content)
              .map(|line| line.to_string())
              .collect()
          });
        on_source(source_index, source, source_content);
      },
      &mut |name_index, name| {
        let mut name_mapping = name_mapping.borrow_mut();
        let len = name_mapping.len() as u32;
        let global_index = match name_mapping.entry(name.to_string()) {
          Entry::Occupied(entry) => *entry.get(),
          Entry::Vacant(entry) => {
            entry.insert(len);
            on_name.borrow_mut()(len, name);
            len
          }
        };
        name_index_mapping
          .borrow_mut()
          .insert(name_index as usize, global_index);
      },
    );

    // Handle remaining replacements
    let mut remainder = String::new();
    while i < repls.len() {
      remainder += &repls[i].content;
      i += 1;
    }

    // Insert remaining replacements content split into chunks by lines
    let mut line = result.generated_line as i64 + generated_line_offset;
    let matches: Vec<&str> = split_into_lines(&remainder).collect();
    for (m, content_line) in matches.iter().enumerate() {
      on_chunk(
        Some(content_line),
        Mapping {
          generated_line: line as u32,
          generated_column: ((result.generated_column as i64)
            + if line == generated_column_offset_line {
              generated_column_offset
            } else {
              0
            }) as u32,
          original: None,
        },
      );

      if m == matches.len() - 1 && !content_line.ends_with('\n') {
        if generated_column_offset_line == line {
          generated_column_offset += content_line.len() as i64;
        } else {
          generated_column_offset = content_line.len() as i64;
          generated_column_offset_line = line;
        }
      } else {
        generated_line_offset += 1;
        line += 1;
        generated_column_offset = -(result.generated_column as i64);
        generated_column_offset_line = line;
      }
    }

    GeneratedInfo {
      generated_line: line as u32,
      generated_column: ((result.generated_column as i64)
        + if line == generated_column_offset_line {
          generated_column_offset
        } else {
          0
        }) as u32,
    }
  }
}

impl<T: Source> Clone for ReplaceSource<T> {
  fn clone(&self) -> Self {
    Self {
      inner: self.inner.clone(),
      inner_source_code: self.inner_source_code.clone(),
      replacements: Mutex::new(self.replacements().clone()),
      is_sorted: AtomicBool::new(self.is_sorted.load(Ordering::SeqCst)),
    }
  }
}

impl<T: Hash> Hash for ReplaceSource<T> {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.sort_replacement();
    "ReplaceSource".hash(state);
    for repl in self.replacements().iter() {
      repl.hash(state);
    }
    self.inner.hash(state);
  }
}

impl<T: PartialEq> PartialEq for ReplaceSource<T> {
  fn eq(&self, other: &Self) -> bool {
    self.inner == other.inner && *self.replacements() == *other.replacements()
  }
}

impl<T: Eq> Eq for ReplaceSource<T> {}

#[cfg(test)]
mod tests {
  use crate::{
    source_map_source::WithoutOriginalOptions, OriginalSource, RawSource,
    ReplacementEnforce, SourceExt, SourceMapSource,
  };

  use super::*;

  fn with_readable_mappings(sourcemap: &SourceMap) -> String {
    let mut first = true;
    let mut last_line = 0;
    sourcemap
      .decoded_mappings()
      .into_iter()
      .map(|token| {
        format!(
          "{}:{} ->{} {}:{}{}",
          if !first && token.generated_line == last_line {
            ", ".to_owned()
          } else {
            first = false;
            last_line = token.generated_line;
            format!("\n{}", token.generated_line)
          },
          token.generated_column,
          token
            .original
            .as_ref()
            .and_then(
              |original| sourcemap.get_source(original.source_index as usize)
            )
            .map_or("".to_owned(), |source| format!(" [{source}]")),
          token
            .original
            .as_ref()
            .map(|original| original.original_line)
            .unwrap_or(!0),
          token
            .original
            .as_ref()
            .map(|original| original.original_column)
            .unwrap_or(!0),
          token
            .original
            .as_ref()
            .and_then(|original| original.name_index)
            .and_then(|name_index| sourcemap.get_name(name_index as usize))
            .map_or("".to_owned(), |source| format!(" ({source})")),
        )
      })
      .collect()
  }

  #[test]
  fn should_replace_correctly() {
    let line1 = "Hello World!";
    let line2 = "{}";
    let line3 = "Line 3";
    let line4 = "Line 4";
    let line5 = "Line 5";
    let code = [line1, line2, line3, line4, line5, "Last", "Line"].join("\n");
    let mut source =
      ReplaceSource::new(OriginalSource::new(code.as_str(), "file.txt"));

    let start_line3 = (line1.len() + line2.len() + 2) as u32;
    let start_line6 =
      (start_line3 as usize + line3.len() + line4.len() + line5.len() + 3)
        as u32;
    source.replace(start_line3, start_line6, "", None);
    source.replace(1, 5, "i ", None);
    source.replace(1, 5, "bye", None);
    source.replace(7, 8, "0000", None);
    source.insert((line1.len() + 2) as u32, "\n Multi Line\n", None);
    source.replace(start_line6 + 4, start_line6 + 5, " ", None);

    let result = source.source();
    let result_map = source.map(&MapOptions::default()).unwrap();

    assert_eq!(
      code,
      r#"Hello World!
{}
Line 3
Line 4
Line 5
Last
Line"#
    );

    assert_eq!(
      result,
      r#"Hi bye W0000rld!
{
 Multi Line
}
Last Line"#
    );

    assert_eq!(
      with_readable_mappings(&result_map),
      r#"
1:0 -> [file.txt] 1:0, :1 -> [file.txt] 1:1, :3 -> [file.txt] 1:5, :8 -> [file.txt] 1:7, :12 -> [file.txt] 1:8
2:0 -> [file.txt] 2:0, :1 -> [file.txt] 2:1
3:0 -> [file.txt] 2:1
4:0 -> [file.txt] 2:1
5:0 -> [file.txt] 6:0, :4 -> [file.txt] 6:4, :5 -> [file.txt] 7:0"#
    );

    let result_list_map = source.map(&MapOptions::new(false)).unwrap();
    assert_eq!(
      with_readable_mappings(&result_list_map),
      r#"
1:0 -> [file.txt] 1:0
2:0 -> [file.txt] 2:0
3:0 -> [file.txt] 2:0
4:0 -> [file.txt] 2:0
5:0 -> [file.txt] 6:0"#
    );
  }

  #[test]
  fn should_replace_multiple_items_correctly() {
    let line1 = "Hello";
    let mut source = ReplaceSource::new(OriginalSource::new(
      ["Hello", "World!"].join("\n").as_str(),
      "file.txt",
    ));
    let original_code = source.source().to_string();
    source.insert(0, "Message: ", None);
    source.replace(2, (line1.len() + 5) as u32, "y A", None);
    let result_text = source.source();
    let result_map = source.map(&MapOptions::default()).unwrap();
    let result_list_map = source.map(&MapOptions::new(false)).unwrap();

    assert_eq!(
      original_code,
      r#"Hello
World!"#
    );
    assert_eq!(result_text, "Message: Hey Ad!");
    assert_eq!(
      with_readable_mappings(&result_map),
      r#"
1:0 -> [file.txt] 1:0, :11 -> [file.txt] 1:2, :14 -> [file.txt] 2:4"#
    );
    assert_eq!(
      with_readable_mappings(&result_list_map),
      r#"
1:0 -> [file.txt] 1:0"#
    );
    assert_eq!(result_map.mappings(), "AAAA,WAAE,GACE");
    assert_eq!(result_list_map.mappings(), "AAAA");
  }

  #[test]
  fn should_prepend_items_correctly() {
    let mut source =
      ReplaceSource::new(OriginalSource::new("Line 1", "file.txt"));
    source.insert(0, "Line -1\n", None);
    source.insert(0, "Line 0\n", None);

    let result_text = source.source();
    let result_map = source.map(&MapOptions::default()).unwrap();
    let result_list_map = source.map(&MapOptions::new(false)).unwrap();

    assert_eq!(result_text, "Line -1\nLine 0\nLine 1");
    assert_eq!(
      with_readable_mappings(&result_map),
      r#"
1:0 -> [file.txt] 1:0
2:0 -> [file.txt] 1:0
3:0 -> [file.txt] 1:0"#
    );
    assert_eq!(
      with_readable_mappings(&result_list_map),
      r#"
1:0 -> [file.txt] 1:0
2:0 -> [file.txt] 1:0
3:0 -> [file.txt] 1:0"#
    );
  }

  #[test]
  fn should_prepend_items_with_replace_at_start_correctly() {
    let mut source = ReplaceSource::new(OriginalSource::new(
      ["Line 1", "Line 2"].join("\n").as_str(),
      "file.txt",
    ));
    source.insert(0, "Line 0\n", None);
    source.replace(0, 6, "Hello", None);
    let result_text = source.source();
    let result_map = source.map(&MapOptions::default()).unwrap();
    let result_list_map = source.map(&MapOptions::new(false)).unwrap();

    assert_eq!(
      result_text,
      r#"Line 0
Hello
Line 2"#
    );
    assert_eq!(
      result_map.to_json().unwrap(),
      r#"{"version":3,"sources":["file.txt"],"sourcesContent":["Line 1\nLine 2"],"names":[],"mappings":"AAAA;AAAA,KAAM;AACN"}"#
    );
    assert_eq!(
      result_list_map.to_json().unwrap(),
      r#"{"version":3,"sources":["file.txt"],"sourcesContent":["Line 1\nLine 2"],"names":[],"mappings":"AAAA;AAAA;AACA"}"#
    );
  }

  #[test]
  fn should_append_items_correctly() {
    let line1 = "Line 1\n";
    let mut source = ReplaceSource::new(OriginalSource::new(line1, "file.txt"));
    source.insert((line1.len() + 1) as u32, "Line 2\n", None);
    let result_text = source.source();
    let result_map = source.map(&MapOptions::default()).unwrap();
    let result_list_map = source.map(&MapOptions::new(false)).unwrap();

    assert_eq!(result_text, "Line 1\nLine 2\n");
    assert_eq!(
      result_map.to_json().unwrap(),
      r#"{"version":3,"sources":["file.txt"],"sourcesContent":["Line 1\n"],"names":[],"mappings":"AAAA"}"#
    );
    assert_eq!(
      result_list_map.to_json().unwrap(),
      r#"{"version":3,"sources":["file.txt"],"sourcesContent":["Line 1\n"],"names":[],"mappings":"AAAA"}"#
    );
  }

  #[test]
  fn should_produce_correct_source_map() {
    let bootstrap_code = "   var hello\n   var world\n";
    let mut source =
      ReplaceSource::new(OriginalSource::new(bootstrap_code, "file.js"));
    source.replace(7, 12, "h", Some("hello"));
    source.replace(20, 25, "w", Some("world"));
    let result_map = source.map(&MapOptions::default()).expect("failed");

    let target_code = source.source();
    assert_eq!(target_code, "   var h\n   var w\n");

    assert_eq!(
      with_readable_mappings(&result_map),
      r#"
1:0 -> [file.js] 1:0, :7 -> [file.js] 1:7 (hello), :8 -> [file.js] 1:12
2:0 -> [file.js] 2:0, :7 -> [file.js] 2:7 (world), :8 -> [file.js] 2:12"#
    );
    assert_eq!(
      result_map.to_json().unwrap(),
      r#"{"version":3,"sources":["file.js"],"sourcesContent":["   var hello\n   var world\n"],"names":["hello","world"],"mappings":"AAAA,OAAOA,CAAK;AACZ,OAAOC,CAAK"}"#
    );
  }

  #[test]
  fn should_allow_replacements_at_the_start() {
    let map = SourceMap::from_slice(
      r#"{
        "version":3,
        "sources":["abc"],
        "names":["StaticPage","data","foo"],
        "mappings":";;AAAA,eAAe,SAASA,UAAT,OAA8B;AAAA,MAARC,IAAQ,QAARA,IAAQ;AAC3C,sBAAO;AAAA,cAAMA,IAAI,CAACC;AAAX,IAAP;AACD",
        "sourcesContent":["export default function StaticPage({ data }) {\nreturn <div>{data.foo}</div>\n}\n"],
        "file":"x"
      }"#.as_bytes(),
    ).unwrap();

    let code = r#"import { jsx as _jsx } from "react/jsx-runtime";
export var __N_SSG = true;
export default function StaticPage(_ref) {
  var data = _ref.data;
  return /*#__PURE__*/_jsx("div", {
    children: data.foo
  });
}"#;

    /*
      3:0 -> [abc] 1:0, :15 -> [abc] 1:15, :24 -> [abc] 1:24 (StaticPage), :34 -> [abc] 1:15, :41 -> [abc] 1:45
      4:0 -> [abc] 1:45, :6 -> [abc] 1:37 (data), :10 -> [abc] 1:45, :18 -> [abc] 1:37 (data), :22 -> [abc] 1:45
      5:0 -> [abc] 2:2, :22 -> [abc] 2:9
      6:0 -> [abc] 2:9, :14 -> [abc] 2:15 (data), :18 -> [abc] 2:19, :19 -> [abc] 2:20 (foo)
      7:0 -> [abc] 2:9, :4 -> [abc] 2:2
      8:0 -> [abc] 3:1
    */

    let mut source =
      ReplaceSource::new(SourceMapSource::new(WithoutOriginalOptions {
        value: code,
        name: "source.js",
        source_map: map,
      }));
    source.replace(0, 48, "", None);
    source.replace(49, 56, "", None);
    source.replace(76, 91, "", None);
    source.replace(
      165,
      169,
      "(0,react_jsx_runtime__WEBPACK_IMPORTED_MODULE_0__.jsx)",
      None,
    );

    let target_code = source.source();
    let source_map = source.map(&MapOptions::default()).unwrap();

    assert_eq!(
      target_code,
      r#"
var __N_SSG = true;
function StaticPage(_ref) {
  var data = _ref.data;
  return /*#__PURE__*/(0,react_jsx_runtime__WEBPACK_IMPORTED_MODULE_0__.jsx)("div", {
    children: data.foo
  });
}"#
    );
    assert_eq!(source_map.get_name(0).unwrap(), "StaticPage");
    assert_eq!(source_map.get_name(1).unwrap(), "data");
    assert_eq!(source_map.get_name(2).unwrap(), "foo");
    assert_eq!(
      source_map.get_source_content(0).unwrap(),
      r#"export default function StaticPage({ data }) {
return <div>{data.foo}</div>
}
"#
    );
    assert!(source_map.file().is_none());
    assert_eq!(source_map.get_source(0).unwrap(), "abc");

    assert_eq!(
      with_readable_mappings(&source_map),
      r#"
3:0 -> [abc] 1:15, :9 -> [abc] 1:24 (StaticPage), :19 -> [abc] 1:15, :26 -> [abc] 1:45
4:0 -> [abc] 1:45, :6 -> [abc] 1:37 (data), :10 -> [abc] 1:45, :18 -> [abc] 1:37 (data), :22 -> [abc] 1:45
5:0 -> [abc] 2:2, :22 -> [abc] 2:9
6:0 -> [abc] 2:9, :14 -> [abc] 2:15 (data), :18 -> [abc] 2:19, :19 -> [abc] 2:20 (foo)
7:0 -> [abc] 2:9, :4 -> [abc] 2:2
8:0 -> [abc] 3:1"#
    );
  }

  #[test]
  fn should_not_generate_invalid_mappings_when_replacing_multiple_lines_of_code(
  ) {
    let mut source = ReplaceSource::new(OriginalSource::new(
      r#"if (a;b;c) {
  a; b; c;
}"#,
      "document.js",
    ));
    source.replace(4, 9, "false", None);
    source.replace(12, 24, "", None);

    let target_code = source.source();
    let source_map = source.map(&MapOptions::default()).unwrap();

    assert_eq!(target_code, "if (false) {}");
    assert_eq!(
      with_readable_mappings(&source_map),
      r#"
1:0 -> [document.js] 1:0, :4 -> [document.js] 1:4, :9 -> [document.js] 1:9, :12 -> [document.js] 3:0"#
    );
    assert_eq!(
      source_map.to_json().unwrap(),
      r#"{"version":3,"sources":["document.js"],"sourcesContent":["if (a;b;c) {\n  a; b; c;\n}"],"names":[],"mappings":"AAAA,IAAI,KAAK,GAET"}"#
    );
  }

  #[test]
  fn test_edge_case() {
    let line1 = "hello world\n";
    let mut source = ReplaceSource::new(OriginalSource::new(line1, "file.txt"));

    source.insert(0, "start1\n", None);
    source.replace(0, 0, "start2\n", None);
    source.replace(999, 10000, "end2", None);
    source.insert(888, "end1\n", None);
    source.replace(0, 999, "replaced!\n", Some("whole"));

    let result_text = source.source();
    let result_map = source.map(&MapOptions::default()).unwrap();

    assert_eq!(result_text, "start1\nstart2\nreplaced!\nend1\nend2");
    assert_eq!(
      with_readable_mappings(&result_map),
      r#"
1:0 -> [file.txt] 1:0
2:0 -> [file.txt] 1:0
3:0 -> [file.txt] 1:0 (whole)"#
    );
  }

  #[test]
  fn replace_source_over_a_box_source() {
    let mut source = ReplaceSource::new(RawSource::from("boxed").boxed());
    source.replace(3, 5, "", None);
    assert_eq!(source.size(), 3);
    assert_eq!(source.source(), "box");
    assert_eq!(source.map(&MapOptions::default()), None);
    let mut hasher = twox_hash::XxHash64::default();
    source.hash(&mut hasher);
    assert_eq!(format!("{:x}", hasher.finish()), "5781cda25d360a42");
  }

  #[test]
  fn should_replace_correctly_with_unicode() {
    let content = r#"
"abc"; url(__PUBLIC_PATH__logo.png);
"ヒラギノ角ゴ"; url(__PUBLIC_PATH__logo.png);
"游ゴシック体"; url(__PUBLIC_PATH__logo.png);
"🤪"; url(__PUBLIC_PATH__logo.png);
"👨‍👩‍👧‍👧"; url(__PUBLIC_PATH__logo.png);
"#;
    let mut source =
      ReplaceSource::new(OriginalSource::new(content, "file.css").boxed());
    for mat in regex::Regex::new("__PUBLIC_PATH__")
      .unwrap()
      .find_iter(content)
    {
      source.replace(mat.start() as u32, mat.end() as u32, "../", None);
    }
    assert_eq!(
      source.source(),
      r#"
"abc"; url(../logo.png);
"ヒラギノ角ゴ"; url(../logo.png);
"游ゴシック体"; url(../logo.png);
"🤪"; url(../logo.png);
"👨‍👩‍👧‍👧"; url(../logo.png);
"#
    );
    assert_eq!(
      source
        .map(&MapOptions::default())
        .unwrap()
        .to_json()
        .unwrap(),
      r#"{"version":3,"sources":["file.css"],"sourcesContent":["\n\"abc\"; url(__PUBLIC_PATH__logo.png);\n\"ヒラギノ角ゴ\"; url(__PUBLIC_PATH__logo.png);\n\"游ゴシック体\"; url(__PUBLIC_PATH__logo.png);\n\"🤪\"; url(__PUBLIC_PATH__logo.png);\n\"👨‍👩‍👧‍👧\"; url(__PUBLIC_PATH__logo.png);\n"],"names":[],"mappings":";AACA,OAAO,IAAI,GAAe;AAC1B,sBAAsB;AACtB,sBAAsB;AACtB,QAAQ;AACR,6BAA6B"}"#,
    );
  }

  #[test]
  fn replace_same_position_with_enforce() {
    // Enforce sort HarmonyExportExpressionDependency after PureExpressionDependency, to generate valid code
    let mut source =
      ReplaceSource::new(RawSource::from("export default foo;aaa").boxed());
    let mut source2 = source.clone();
    source.replace(18, 19, ");", None);
    source.replace(18, 19, "))", None);
    assert_eq!(source.source(), "export default foo);))aaa");

    source2.replace_with_enforce(18, 19, ");", None, ReplacementEnforce::Post);
    source2.replace(18, 19, "))", None);
    assert_eq!(source2.source(), "export default foo)));aaa");
  }
}
