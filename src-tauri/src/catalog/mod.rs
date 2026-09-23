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
    /// GGUF family handlers exposed by transcribe-cpp.
    Parakeet,
    Moonshine,
    Canary,
    SenseVoice,
    GigaAm,
    Cohere,
    #[serde(rename = "qwen3_asr")]
    Qwen3Asr,
}

impl Architecture {
    /// Every architecture currently shipped through transcribe-cpp is one GGUF
    /// file. This remains explicit so acquisition/storage can reject a future
    /// directory-shaped entry instead of silently treating it as a file.
    pub fn is_single_file(self) -> bool {
        matches!(
            self,
            Architecture::Whisper
                | Architecture::Parakeet
                | Architecture::Moonshine
                | Architecture::Canary
                | Architecture::SenseVoice
                | Architecture::GigaAm
                | Architecture::Cohere
                | Architecture::Qwen3Asr
        )
    }
}

/// The job a catalog model is recommended for. This is deliberately a small,
/// stable vocabulary: a role is a product promise, not a benchmark class.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelRole {
    /// A compact model that still handles genuinely multilingual dictation.
    MultilingualFast,
    /// The strongest English-focused choice in the catalog.
    EnglishQuality,
    /// The strongest multilingual choice in the catalog.
    MultilingualQuality,
    /// Valid fallback for older catalog data that predates explicit roles.
    #[default]
    General,
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
pub struct License {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HostProvider {
    GithubReleases,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HostStatus {
    Active,
    Planned,
}

/// Bard's intended distribution metadata is explicit and travels with the
/// catalog row. It is separate from `file.url`: a model can remain downloadable
/// from its canonical upstream while bard's first-party mirror is being staged.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstPartyHost {
    pub provider: HostProvider,
    /// GitHub `owner/repository`, without a URL or API prefix.
    pub repository: String,
    #[serde(default)]
    pub release_tag: Option<String>,
    pub asset: String,
    pub url: String,
    pub status: HostStatus,
}

/// Where a model came from, recorded so a catalog row can be traced back to
/// an exact upstream commit rather than a URL that may have moved.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub repo: String,
    pub revision: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub upstream_repo: Option<String>,
    #[serde(default)]
    pub upstream_revision: Option<String>,
    #[serde(default)]
    pub upstream_url: Option<String>,
    #[serde(default)]
    pub license: Option<License>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub role: ModelRole,
    pub architecture: Architecture,
    pub family: String,
    pub parameters: String,
    pub languages: u32,
    pub capabilities: Capabilities,
    /// 0–100, relative to the other entries here rather than to a benchmark.
    pub accuracy: u32,
    pub speed: u32,
    /// 0–100, relative value at the catalog's quality/size trade-off.
    #[serde(default)]
    pub value: u32,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub recommended_rank: Option<u32>,
    pub file: CatalogFile,
    #[serde(default)]
    pub source: Option<Source>,
    #[serde(default)]
    pub host: Option<FirstPartyHost>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_contract_is_complete() {
        assert_eq!(models().len(), 3, "Bard ships exactly three curated defaults");
        let mut ids = HashSet::new();
        let mut roles = HashSet::new();
        let mut filenames = HashSet::new();
        for model in models() {
            assert!(ids.insert(model.id.as_str()), "duplicate id {}", model.id);
            assert!(
                !model.role.eq(&ModelRole::General),
                "{} has no role",
                model.id
            );
            assert!(roles.insert(model.role), "duplicate role in catalog");
            assert!(model.accuracy <= 100 && model.speed <= 100);
            assert!(model.value <= 100);
            assert!(model.architecture.is_single_file());
            assert!(filenames.insert(model.file.filename.as_str()), "duplicate filename {}", model.file.filename);
            assert!(model.file.size_bytes > 0, "{} has no byte size", model.id);
            assert_eq!(model.file.sha256.len(), 64, "{} has no SHA-256", model.id);
            assert!(model.file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert!(model.file.url.starts_with("https://"), "{} is not HTTPS", model.id);
            assert!(model.file.quant.is_some(), "{} has no quant", model.id);
            let source = model
                .source
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no upstream source", model.id));
            assert!(!source.repo.is_empty());
            assert!(!source.revision.is_empty());
            if source.upstream_repo.is_some() {
                assert!(source.upstream_revision.is_some());
                assert!(source.upstream_url.is_some());
            }
            assert!(
                source.license.is_some(),
                "{} has no model license",
                model.id
            );
            let host = model
                .host
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no first-party host", model.id));
            assert_eq!(host.repository, "miisodev/bard-models");
            assert_eq!(host.release_tag.as_deref(), Some("models-v2"));
            assert_eq!(host.status, HostStatus::Active);
            assert_eq!(host.asset, model.file.filename);
            assert_eq!(host.url, model.file.url);
        }
    }
}
