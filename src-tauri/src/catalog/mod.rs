use serde::{Deserialize, Serialize};

/*
  The curated model catalog.

  Baked in at compile time rather than fetched: the list a new user first sees
  has to be there on a machine that has never been online, and a catalog that
  can fail to load is a catalog that can leave the app with nothing to offer.
  Updating it is shipping a build, which is the right cadence for a list whose
  entries carry checksums.

  `architecture` is the load-bearing field. It decides which engine loads a
  model, and nothing in a model's own files states it reliably — so it is
  recorded here or it is guessed at, and guessing is how a model installs
  cleanly and then fails at load.
*/

const CATALOG_JSON: &str = include_str!("catalog.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Architecture {
    /// GGUF / ggml, a single file, loaded by transcribe-cpp.
    Whisper,
    /// The ONNX families, each a *directory* of files, loaded by transcribe-rs.
    Parakeet,
    Moonshine,
    Canary,
    SenseVoice,
    GigaAm,
    Cohere,
}

impl Architecture {
    /// Whisper-family models are one file; every ONNX family is a directory.
    /// Acquisition, storage and loading all branch on this.
    pub fn is_single_file(self) -> bool {
        matches!(self, Architecture::Whisper)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub streaming: bool,
    pub translate: bool,
    pub language_detect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFile {
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub quant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub architecture: Architecture,
    pub family: String,
    pub parameters: String,
    pub languages: u32,
    pub capabilities: Capabilities,
    /// 0–100, relative to the other entries here rather than to a benchmark.
    pub accuracy: u32,
    pub speed: u32,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub recommended_rank: Option<u32>,
    pub file: CatalogFile,
}

#[derive(Debug, Deserialize)]
struct Catalog {
    models: Vec<CatalogModel>,
}

/// The catalog, parsed once. A malformed catalog is a build-time mistake, so
/// this panics rather than degrading to an empty list — an app that silently
/// offers no models is harder to diagnose than one that refuses to start.
pub fn models() -> &'static [CatalogModel] {
    use std::sync::OnceLock;
    static PARSED: OnceLock<Vec<CatalogModel>> = OnceLock::new();
    PARSED
        .get_or_init(|| {
            let catalog: Catalog = serde_json::from_str(CATALOG_JSON)
                .expect("catalog.json is malformed — this is a build-time error");
            catalog.models
        })
        .as_slice()
}

pub fn find(id: &str) -> Option<&'static CatalogModel> {
    models().iter().find(|m| m.id == id)
}

/// Recommended first in rank order, then everything else as listed.
pub fn ordered() -> Vec<&'static CatalogModel> {
    let mut out: Vec<&CatalogModel> = models().iter().collect();
    out.sort_by_key(|m| match (m.recommended, m.recommended_rank) {
        (true, Some(rank)) => (0, rank),
        (true, None) => (0, u32::MAX),
        (false, _) => (1, 0),
    });
    out
}
