use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImageStorage {
    #[default]
    Local,
    Cloud,
    Both,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ImageSettings {
    pub root: Option<String>,
    pub storage: ImageStorage,
}

pub fn validate_images(settings: &ImageSettings) -> Result<()> {
    if let Some(root) = &settings.root {
        ensure!(
            !root.trim().is_empty() && root.len() <= 4096 && !root.chars().any(char::is_control),
            "图片目录须为有效绝对路径"
        );
        ensure!(
            std::path::Path::new(root).is_absolute()
                && !std::path::Path::new(root)
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir)),
            "图片目录须为绝对路径"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_images_default_to_local_ownership() {
        let images: ImageSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(images, ImageSettings::default());
    }
    #[test]
    fn custom_images_location_rejects_traversal_and_relative_paths() {
        for root in ["", "relative/images", "a/../images", "\0"] {
            assert!(
                validate_images(&ImageSettings {
                    root: Some(root.into()),
                    ..Default::default()
                })
                .is_err()
            );
        }
        let root = std::env::temp_dir().join("WWStudyImages");
        assert!(
            validate_images(&ImageSettings {
                root: Some(root.to_string_lossy().into_owned()),
                storage: ImageStorage::Both
            })
            .is_ok()
        );
        let traversal = root.join("..").join("elsewhere");
        assert!(
            validate_images(&ImageSettings {
                root: Some(traversal.to_string_lossy().into_owned()),
                ..Default::default()
            })
            .is_err()
        );
    }
}
