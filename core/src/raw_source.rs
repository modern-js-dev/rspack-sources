use parcel_sourcemap::SourceMap;
use smol_str::SmolStr;

use crate::{utils::Lrc, Error, MapOptions, Source};

pub struct RawSource {
  source_code: SmolStr,
}

impl RawSource {
  pub fn new(source_code: &str) -> Self {
    Self {
      source_code: source_code.into(),
    }
  }

  pub fn from_slice(source_code: &[u8]) -> Result<Self, Error> {
    Ok(Self {
      source_code: String::from_utf8(source_code.to_vec())?.into(),
    })
  }
}

impl Source for RawSource {
  #[tracing::instrument(skip_all)]
  fn map(&mut self, _option: &MapOptions) -> Option<SourceMap> {
    None
  }

  fn source(&mut self) -> String {
    self.source_code.to_string()
  }

  fn size(&self) -> usize {
    self.source_code.len()
  }
}
