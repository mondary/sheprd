mod en_xa;

#[cfg(feature = "moonframe")]
#[gpui::test]
fn status_update_opens_the_update_panel(cx: &mut gpui::TestAppContext) {
    use crate::{
        menu::Page,
        sidebar::layout_tests::{fixture_window, full_draw},
    };

    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.check_from_status(window, cx));
    });
    cx.update(|window, cx| {
        assert_eq!(view.read(cx).menu.page, Some(Page::AppUpdate));
        assert!(view.read(cx).update_preview.is_none());
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("app-update-panel").is_some());
    assert!(cx.debug_bounds("about").is_none());
    assert_eq!(
        cx.debug_bounds("app-update-official-result").is_some(),
        crate::APP_VERSION == "dev"
    );
}
