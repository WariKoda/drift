use super::*;
use gpui_kit::assets::{Assets, IconName};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{AssetSource, TestAppContext};

#[test]
fn typography_and_density_leave_room_for_native_controls() {
    assert_eq!(Text::Body.rems(), rems(0.875));
    assert_eq!(Text::Metadata.rems(), rems(0.75));
    assert_eq!(Text::Title.rems(), rems(1.));
    assert_eq!(size::ROW, 24.);
    assert_eq!(size::PROJECT_FORM, 560.);
    const {
        assert!(size::ROW >= size::ICON + 2. * space::TIGHT);
        assert!(size::HEADER >= size::CONTROL + 2. * space::TIGHT);
    }
    assert_eq!(space::INDENT, 4. * space::TIGHT);
}

#[gpui_kit::test]
fn geometry_uses_the_existing_kit_font_scale_exactly_once(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        for font in [16., 20., 24., 32.] {
            Theme::update(cx, |theme| theme.font_size = gpui_kit::px(font));
            for token in [
                space::TIGHT,
                space::CONTROL,
                space::PANEL,
                space::INDENT,
                size::ROW,
                size::CONTROL,
                size::HEADER,
                size::ICON,
                size::DISCLOSURE,
                size::FILTER,
                size::PROJECT_FORM,
                size::RADIUS,
            ] {
                assert_eq!(metric(token, cx), gpui_kit::px(token * font / 16.));
            }
        }
    });
}

#[test]
fn pilot_icons_are_already_in_the_default_kit_bundle() {
    for icon in [
        IconName::Folder,
        IconName::FolderClosed,
        IconName::FolderOpen,
        IconName::File,
        IconName::FileText,
        IconName::Network,
        IconName::Globe,
        IconName::HardDrive,
        IconName::Replace,
        IconName::ArrowLeft,
        IconName::ArrowRight,
        IconName::ArrowUp,
        IconName::RefreshCw,
        IconName::Search,
        IconName::Eye,
        IconName::EyeOff,
        IconName::Close,
    ] {
        assert!(Assets.load(&icon.path()).unwrap().is_some(), "{icon:?}");
    }
}

#[gpui_kit::test]
fn semantic_palette_follows_both_kit_modes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            Theme::change(mode, None, cx);
            let palette = Palette::current(cx);
            let theme = cx.theme();
            assert_eq!(palette.canvas, theme.background);
            assert_eq!(palette.chrome, theme.list_head);
            assert_eq!(palette.text, theme.foreground);
            assert_eq!(palette.selected, theme.list_active);
            assert_eq!(palette.hover, theme.list_hover);
            assert_eq!(palette.border, theme.border);
            assert_eq!(palette.muted, theme.muted_foreground);
            assert_eq!(palette.focus, theme.ring);
            assert_ne!(palette.focus, palette.selected);
        }
    });
}
