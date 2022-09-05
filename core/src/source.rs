use parcel_sourcemap::SourceMap;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MapOptions {
  /// If set to false the implementation may omit mappings for columns. (default: true)
  pub columns: bool,
  pub include_source_contents: bool,
  pub file: Option<String>,
}

impl Default for MapOptions {
  fn default() -> Self {
    Self {
      columns: true,
      include_source_contents: true,
      file: Default::default(),
    }
  }
}

pub trait Source {
  fn map(&mut self, option: &MapOptions) -> Option<SourceMap>;

  fn source(&mut self) -> String;

  fn size(&self) -> usize;
}

pub type BoxSource = Box<dyn Source>;

impl<S: Source + 'static> From<S> for BoxSource {
  fn from(source: S) -> Self {
    Box::new(source)
  }
}

impl Source for &str {
  fn source(&mut self) -> String {
    self.to_string()
  }

  fn map(&mut self, _: &MapOptions) -> Option<SourceMap> {
    None
  }

  fn size(&self) -> usize {
    self.len()
  }
}

impl Source for SourceMap {
  fn map(&mut self, _: &MapOptions) -> Option<SourceMap> {
    None
  }

  fn source(&mut self) -> String {
    self.to_json(None).unwrap()
  }

  fn size(&self) -> usize {
    0 // TODO
  }
}
