use gpui_kit::assets::{Assets as KitAssets, icon_assets};
use gpui_kit::{AssetSource, Result, SharedString};
use std::borrow::Cow;

// Only icons outside the default Kit bundle; the full catalog stays unembedded.
icon_assets!(ExtraIcons, [House, Unplug]);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match ExtraIcons.load(path)? {
            Some(icon) => Ok(Some(icon)),
            None => KitAssets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = KitAssets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::assets::IconName;

    #[test]
    fn only_house_and_unplug_are_extra_assets_and_kit_icons_remain_unchanged() {
        let original = KitAssets.list("").unwrap();
        let combined = Assets.list("").unwrap();
        assert_eq!(combined.len(), original.len() + 2);
        for icon in [IconName::House, IconName::Unplug] {
            assert!(combined.contains(&icon.path()));
            assert!(Assets.load(&icon.path()).unwrap().is_some());
        }
        for path in original {
            assert_eq!(Assets.load(&path).unwrap(), KitAssets.load(&path).unwrap());
        }
        assert!(!combined.contains(&IconName::Camera.path()));
        assert!(
            Assets
                .list("icons/house")
                .unwrap()
                .iter()
                .all(|path| path.starts_with("icons/house"))
        );
    }
}
