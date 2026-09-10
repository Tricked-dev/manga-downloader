//! Automatic model selection.
//!
//! MangaJaNai ships one model per source resolution because halftone frequency
//! depends on the resolution of the original scan, so picking the right one
//! matters. The rule implemented here is transcribed from
//! MangaJaNaiConverterGui's default workflow rather than invented; see
//! `docs/REFERENCE-SEMANTICS.md`. In particular the source-height bands are
//! *not* centred on the model names -- the 1600p model covers 1551..1760.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

/// A model available for selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    /// Path to the `.onnx` file, relative to the manifest.
    pub path: PathBuf,
    #[serde(default)]
    pub architecture: String,
    pub scale: u32,
    #[serde(default = "default_channels")]
    pub input_channels: usize,
    /// Source height the model was trained for, purely descriptive.
    #[serde(default)]
    pub target_height: Option<u32>,
    /// Inclusive lower bound of the source heights this model serves.
    /// `Some(0)` means unbounded; `None` excludes it from automatic selection.
    #[serde(default)]
    pub source_height_min: Option<u32>,
    /// Inclusive upper bound. `Some(0)` means unbounded.
    #[serde(default)]
    pub source_height_max: Option<u32>,
}

fn default_channels() -> usize {
    3
}

impl ModelEntry {
    /// Whether this model serves pages of the given height.
    ///
    /// Mirrors `should_chain_activate_for_image`: a bound of 0 means
    /// unbounded, and both bounds are inclusive.
    pub fn serves_height(&self, height: u32) -> bool {
        let (Some(min), Some(max)) = (self.source_height_min, self.source_height_max) else {
            return false;
        };
        if min != 0 && min > height {
            return false;
        }
        if max != 0 && max < height {
            return false;
        }
        true
    }
}

/// The contents of `models/models.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub version: u32,
    /// Target scales at or below this pick the smaller-scale model.
    #[serde(default = "default_scale_boundary")]
    pub scale_boundary: u32,
    pub models: Vec<ModelEntry>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub comment: Option<String>,
    /// Directory the manifest was loaded from, used to resolve model paths.
    #[serde(skip)]
    root: PathBuf,
}

fn default_scale_boundary() -> u32 {
    2
}

impl ModelManifest {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let mut manifest: Self = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        if manifest.version != 1 {
            bail!(
                "{} is manifest version {}, this build understands version 1",
                path.display(),
                manifest.version
            );
        }
        manifest.root = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Ok(manifest)
    }

    /// Resolve a manifest entry's path against the manifest's directory.
    pub fn resolve(&self, entry: &ModelEntry) -> PathBuf {
        if entry.path.is_absolute() {
            entry.path.clone()
        } else {
            self.root.join(&entry.path)
        }
    }

    /// Pick the model for a page of `height` pixels at the requested scale.
    ///
    /// Upstream lists the 2x chain before the 4x chain for every band and a
    /// target scale of exactly 2 satisfies both, so the boundary is inclusive
    /// towards the smaller model.
    pub fn select(&self, height: u32, target_scale: u32) -> Result<&ModelEntry> {
        let wanted_scale = if target_scale <= self.scale_boundary {
            self.scale_boundary
        } else {
            target_scale
        };

        let candidates: Vec<&ModelEntry> = self
            .models
            .iter()
            .filter(|entry| entry.serves_height(height))
            .collect();

        if candidates.is_empty() {
            bail!(
                "no model in the manifest serves a source height of {height}px. \
                 Export more models with tools/export_onnx.py, or pass an explicit \
                 --model path."
            );
        }

        candidates
            .iter()
            .find(|entry| entry.scale == wanted_scale)
            .copied()
            .ok_or_else(|| {
                let available: Vec<String> = candidates
                    .iter()
                    .map(|entry| format!("{}x ({})", entry.scale, entry.name))
                    .collect();
                anyhow!(
                    "no {wanted_scale}x model for a source height of {height}px; \
                     available for that height: {}",
                    available.join(", ")
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, scale: u32, height: u32, min: u32, max: u32) -> ModelEntry {
        ModelEntry {
            name: name.to_owned(),
            path: PathBuf::from(format!("{name}.onnx")),
            architecture: "ESRGAN".to_owned(),
            scale,
            input_channels: 3,
            target_height: Some(height),
            source_height_min: Some(min),
            source_height_max: Some(max),
        }
    }

    /// The upstream band table, as documented in docs/REFERENCE-SEMANTICS.md.
    fn upstream_manifest() -> ModelManifest {
        let bands = [
            (1200u32, 0u32, 1250u32),
            (1300, 1251, 1350),
            (1400, 1351, 1450),
            (1500, 1451, 1550),
            (1600, 1551, 1760),
            (1920, 1761, 1984),
            (2048, 1985, 0),
        ];
        let mut models = Vec::new();
        for (height, min, max) in bands {
            for scale in [2u32, 4] {
                models.push(entry(
                    &format!("{scale}x_MangaJaNai_{height}p"),
                    scale,
                    height,
                    min,
                    max,
                ));
            }
        }
        ModelManifest {
            version: 1,
            scale_boundary: 2,
            models,
            comment: None,
            root: PathBuf::from("models"),
        }
    }

    #[test]
    fn selection_matches_the_upstream_band_table() {
        let manifest = upstream_manifest();
        // (source height, expected target height) at the band edges.
        let cases = [
            (1u32, 1200u32),
            (1000, 1200),
            (1250, 1200),
            (1251, 1300),
            (1350, 1300),
            (1351, 1400),
            (1450, 1400),
            (1451, 1500),
            (1550, 1500),
            (1551, 1600),
            (1600, 1600),
            (1760, 1600),
            (1761, 1920),
            (1920, 1920),
            (1984, 1920),
            (1985, 2048),
            (4000, 2048),
        ];
        for (height, expected) in cases {
            let selected = manifest.select(height, 4).unwrap();
            assert_eq!(
                selected.target_height,
                Some(expected),
                "height {height} selected {}",
                selected.name
            );
            assert_eq!(selected.scale, 4);
        }
    }

    #[test]
    fn a_target_scale_of_exactly_two_picks_the_2x_model() {
        // Upstream lists the 2x chain first and both chains accept 2.
        let manifest = upstream_manifest();
        assert_eq!(manifest.select(1600, 2).unwrap().scale, 2);
        assert_eq!(manifest.select(1600, 1).unwrap().scale, 2);
        assert_eq!(manifest.select(1600, 4).unwrap().scale, 4);
    }

    #[test]
    fn zero_bounds_mean_unbounded() {
        let manifest = upstream_manifest();
        // The 1200p band has min 0, the 2048p band has max 0.
        assert_eq!(manifest.select(1, 4).unwrap().target_height, Some(1200));
        assert_eq!(
            manifest.select(99_999, 4).unwrap().target_height,
            Some(2048)
        );
    }

    #[test]
    fn a_model_without_bands_is_never_selected_automatically() {
        let mut manifest = upstream_manifest();
        manifest.models.push(ModelEntry {
            source_height_min: None,
            source_height_max: None,
            ..entry("4x_IllustrationJaNai", 4, 0, 0, 0)
        });
        assert_eq!(manifest.select(1600, 4).unwrap().target_height, Some(1600));
    }

    #[test]
    fn a_missing_scale_reports_what_is_available() {
        let manifest = ModelManifest {
            version: 1,
            scale_boundary: 2,
            models: vec![entry("4x_MangaJaNai_1600p", 4, 1600, 1551, 1760)],
            comment: None,
            root: PathBuf::from("models"),
        };
        let error = manifest.select(1600, 2).unwrap_err().to_string();
        assert!(error.contains("4x"), "unhelpful error: {error}");
    }

    #[test]
    fn an_unserved_height_is_an_error_not_a_silent_fallback() {
        let manifest = ModelManifest {
            version: 1,
            scale_boundary: 2,
            models: vec![entry("4x_MangaJaNai_1600p", 4, 1600, 1551, 1760)],
            comment: None,
            root: PathBuf::from("models"),
        };
        assert!(manifest.select(800, 4).is_err());
    }
}
