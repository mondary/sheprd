//! Classic macOS-style About box: build identity only. It reads embedded build
//! constants, never the daemon; links open in the browser. A dev build has no
//! release identity for the updater, so it also shows a read-only check of the
//! official releases, fetched off the UI thread.
// i18n: migrated
use crate::i18n::fl;
use crate::{SheprdWindow, menu::Page};
use gpui::{prelude::*, *};
use sheprd_updater::OfficialRelease;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, LazyLock};

pub(super) const WEBSITE: &str = "https://herdr.dev/";
const HOMEPAGE: &str = "https://sheprd.org/";
pub(super) const REPOSITORY: &str = "https://github.com/sheprd-labs/sheprd";
const COPYRIGHT_YEAR: &str = "© 2026";
// i18n: data
const AUTHOR: &str = "Fabien Penso";
const AUTHOR_URL: &str = "https://pen.so";
const TWITTER_URL: &str = "https://x.com/fabienpenso";
// i18n: data
const LICENSE: &str = "· Apache-2.0";

static ICON: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Png,
        crate::app_icon::png().to_vec(),
    ))
});

/// What About knows about the official releases. A dev build carries no
/// release version, so the updater never starts; this check only reads where
/// upstream is. The request runs on its own thread and the window polls.
#[derive(Default)]
pub(crate) struct OfficialCheck {
    rx: Option<Receiver<sheprd_updater::Result<Vec<OfficialRelease>>>>,
    /// Fetched once per run unless the user asks again, so opening About does
    /// not burn the anonymous GitHub quota.
    started: bool,
    state: OfficialState,
}

#[derive(Default)]
enum OfficialState {
    #[default]
    Idle,
    /// The request is in flight. Unit tests never start one, so the variant
    /// is only constructed outside them.
    #[cfg_attr(test, allow(dead_code))]
    Checking,
    Ready(Vec<OfficialRelease>),
    Failed,
}

impl OfficialCheck {
    pub(crate) fn summary(&self) -> String {
        match &self.state {
            OfficialState::Idle | OfficialState::Checking => fl!("about-official-checking"),
            OfficialState::Failed => fl!("about-official-failed"),
            OfficialState::Ready(releases) => releases.first().map_or_else(
                || fl!("about-official-none"),
                |release| {
                    if release.prerelease {
                        fl!("about-official-prerelease", version = release.tag.clone())
                    } else {
                        fl!("about-official-latest", version = release.tag.clone())
                    }
                },
            ),
        }
    }

    pub(super) fn start_once(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        self.start();
    }

    pub(super) fn start(&mut self) {
        if matches!(self.state, OfficialState::Checking) {
            return;
        }
        // Unit tests never reach GitHub: they seed the state instead.
        #[cfg(test)]
        {
            self.started = true;
        }
        #[cfg(not(test))]
        {
            let (sender, receiver) = std::sync::mpsc::channel();
            let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let spawned = std::thread::Builder::new()
                .name("sheprd-official-releases".into())
                .spawn(move || {
                    let result = sheprd_updater::official_releases(&cancel);
                    let _ = sender.send(result);
                });
            if spawned.is_ok() {
                self.rx = Some(receiver);
                self.state = OfficialState::Checking;
            } else {
                self.state = OfficialState::Failed;
            }
        }
    }

    #[cfg(test)]
    pub(super) fn seed(&mut self, releases: Vec<OfficialRelease>) {
        self.started = true;
        self.rx = None;
        self.state = OfficialState::Ready(releases);
    }

    /// True when the shown line changed, so the window repaints.
    pub(super) fn poll(&mut self) -> bool {
        let Some(receiver) = &self.rx else {
            return false;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.rx = None;
                self.state = match result {
                    Ok(releases) => OfficialState::Ready(releases),
                    Err(_) => OfficialState::Failed,
                };
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                self.state = OfficialState::Failed;
                true
            }
        }
    }
}

impl SheprdWindow {
    pub(super) fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.about_official.start_once();
        self.menu.page = Some(Page::About);
    }

    pub(super) fn render_about(&self, cx: &mut Context<Self>) -> Div {
        let font = &self.config.ui;
        let theme = &self.theme;
        let muted = rgb(theme.muted);
        #[cfg(feature = "moonframe")]
        let version = crate::luau_window::pk_version();
        #[cfg(not(feature = "moonframe"))]
        let version = crate::APP_VERSION;
        div()
            .debug_selector(|| "about".into())
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .px(px(24.))
            .py(px(28.))
            .text_center()
            .child(
                div()
                    .debug_selector(|| "about-icon".into())
                    .size(px(96.))
                    .mb(px(6.))
                    .child(img(ICON.clone()).size_full()),
            )
            .child(
                div()
                    .debug_selector(|| "about-name".into())
                    .text_size(px(font.size * 1.8))
                    .line_height(px(font.size * 2.2))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Sheprd"), // i18n: data
            )
            .child(
                div()
                    .debug_selector(|| "about-version".into())
                    .text_color(muted)
                    .child(fl!("about-version", version = version)),
            )
            .children(self.render_about_dev(theme, cx))
            .child(
                div()
                    .debug_selector(|| "about-summary".into())
                    .pt(px(6.))
                    .text_color(muted)
                    .child(fl!("about-summary")),
            )
            .child(
                div()
                    .debug_selector(|| "about-unaffiliated".into())
                    .pt(px(4.))
                    .text_size(px(font.size * 0.85))
                    .text_color(muted)
                    .child(fl!("about-unaffiliated")),
            )
            .children(self.render_about_build(theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .pt(px(10.))
                    .child(self.about_link("about-homepage", fl!("about-website"), HOMEPAGE))
                    .child(self.about_link("about-website", "herdr.dev", WEBSITE))
                    // i18n: data
                    .child(self.about_link("about-repository", "GitHub", REPOSITORY)),
            )
            .when(cfg!(feature = "moonframe"), |about| {
                about.child(self.about_link("about-support", "Ko-fi", "https://ko-fi.com/pouark"))
            })
            .child(
                div()
                    .debug_selector(|| "about-copyright".into())
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .pt(px(10.))
                    .text_size(px(font.size * 0.85))
                    .text_color(muted)
                    .child(COPYRIGHT_YEAR)
                    .child(self.about_link("about-author", AUTHOR, AUTHOR_URL))
                    .child(
                        self.about_link(
                            "about-twitter",
                            svg()
                                .path("icons/x.svg")
                                .size(px(font.size * 0.85))
                                .text_color(crate::menu::accent(theme)),
                            TWITTER_URL,
                        ),
                    )
                    .child(LICENSE),
            )
            .child(
                div()
                    .id("about-close")
                    .debug_selector(|| "about-close".into())
                    .mt(px(14.))
                    .px(px(16.))
                    .py(px(6.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .bg(rgb(theme.active))
                    .cursor_pointer()
                    .child(fl!("about-ok"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.dismiss_menu(window, cx);
                    })),
            )
    }

    /// A dev build has no release number and no updater: name the commit it
    /// came from and where the official releases stand, read-only.
    fn render_about_dev(
        &self,
        theme: &crate::config::Theme,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        (crate::APP_VERSION == "dev").then(|| {
            let muted = rgb(theme.muted);
            let identity = match crate::constants::BUILD_COMMIT.filter(|commit| !commit.is_empty())
            {
                Some(commit) => fl!("about-dev-commit", commit = commit),
                None => fl!("about-dev-build"),
            };
            div()
                .debug_selector(|| "about-dev".into())
                .flex()
                .flex_col()
                .items_center()
                .gap(px(2.))
                .pt(px(4.))
                .text_size(px(self.config.ui.size * 0.85))
                .text_color(muted)
                .child(
                    div()
                        .debug_selector(|| "about-dev-identity".into())
                        .child(identity),
                )
                .child(
                    div()
                        .debug_selector(|| "about-official".into())
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .pt(px(2.))
                        .child(self.about_official.summary())
                        .child(
                            div()
                                .id("about-official-check")
                                .debug_selector(|| "about-official-check".into())
                                .cursor_pointer()
                                .text_color(crate::menu::accent(theme))
                                .border_b_1()
                                .border_color(transparent_black())
                                .hover(|style| style.border_color(crate::menu::accent(theme)))
                                .child(fl!("about-official-check"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.about_official.start();
                                    cx.notify();
                                })),
                        ),
                )
        })
    }

    /// Worktree builds are throwaway; name the branch that produced this one.
    fn render_about_build(&self, theme: &crate::config::Theme) -> Option<Div> {
        (env!("HERDR_BUILD_WORKTREE") == "1").then(|| {
            let pr = env!("HERDR_BUILD_PR");
            div()
                .debug_selector(|| "about-build".into())
                .flex()
                .items_center()
                .gap(px(8.))
                .pt(px(6.))
                .max_w_full()
                .text_size(px(self.config.ui.size * 0.85))
                .text_color(rgb(theme.muted))
                .child(
                    div()
                        .debug_selector(|| "about-branch".into())
                        .min_w_0()
                        .truncate()
                        .child(env!("HERDR_BUILD_BRANCH")),
                )
                .when(!pr.is_empty(), |row| {
                    row.child(self.about_link(
                        "about-pr",
                        fl!("about-pull-request", number = pr),
                        crate::worktree_banner::pull_request_url(pr),
                    ))
                })
        })
    }

    fn about_link(
        &self,
        id: &'static str,
        label: impl IntoElement,
        url: impl Into<String>,
    ) -> Stateful<Div> {
        let url = url.into();
        div()
            .id(id)
            .debug_selector(move || id.into())
            .role(Role::Link)
            .tab_index(0)
            .focus_visible(|style| style.underline())
            .flex_none()
            .flex()
            .items_center()
            .cursor_pointer()
            .text_color(crate::menu::accent(&self.theme))
            .border_b_1()
            .border_color(transparent_black())
            .hover(|style| style.border_color(crate::menu::accent(&self.theme)))
            .child(label)
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                cx.open_url(&url);
            })
    }
}

#[cfg(test)]
mod tests;
