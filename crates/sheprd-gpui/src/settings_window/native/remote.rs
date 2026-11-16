//! The native capture of Settings > Remote control > Relays with web
//! access: example relays and a daemon report, as drafts only, put back
//! after the capture so the sandbox's files and service are untouched.
// i18n: data

use super::*;
use crate::{
    config::remote::Relay,
    remote::{Health, Supervisor},
};

pub(super) async fn capture_relays(
    settings: WindowHandle<SettingsWindow>,
    wide: bool,
    cx: &mut AsyncApp,
) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    let now = u64::try_from(now)?;
    let relay = |endpoint: &str, name: &str, web: bool| -> Result<Relay> {
        let mut relay = Relay::from(endpoint.to_owned());
        relay.label.name = Some(name.to_owned());
        relay.token_file = web.then(|| std::path::PathBuf::from("/etc/sheprd/relay.token"));
        relay.web = web;
        Ok(relay)
    };
    let relays = vec![
        relay("https://fra1.sheprd-relay.dev", "fra1", true)?,
        relay("https://nyc3.sheprd-relay.dev", "nyc3", true)?,
        relay("https://relay.example.org", "example", false)?,
    ];
    let status: sheprd_remote_control::control::Status = serde_json::from_value(
        serde_json::json!({
            "version": "0.1", "port": 7443, "addresses": [], "fingerprint": "",
            "herdr": true, "chats": 0, "devices": 1, "connected": 0,
            "relays": {
                "https://fra1.sheprd-relay.dev": {
                    "state": "listening", "rtt_ms": 18, "name": "fra1",
                    "web": {
                        "url": "https://n5uw4ztlnvswk5dbnzthk3dfojrxi2lp.fra1.sheprd-relay.dev",
                        "expires_ms": now + 61 * 86_400_000,
                        "transparency": {"state": "clean", "checked_ms": now - 12 * 60_000},
                    },
                },
                "https://nyc3.sheprd-relay.dev": {
                    "state": "listening", "rtt_ms": 84, "name": "nyc3",
                    "web": {
                        "url": "https://mfrggzdfmztwq2lknnwg23tpobyxe43u.nyc3.sheprd-relay.dev",
                        "transparency": {"state": "alert", "certificate": "c0ffee", "found_ms": now - 3_600_000},
                    },
                },
                "https://relay.example.org": {"state": "listening", "rtt_ms": 41, "name": "example"},
            },
        }),
    )?;
    // The fixture runs no remote control: a bare supervisor, which starts
    // nothing, holds the report for the capture and goes after it.
    let (saved, health, installed) = settings.update(cx, |view, _, cx| {
        let saved = std::mem::replace(&mut view.config.remote.relays, relays);
        let health = Supervisor::health(cx);
        let installed = !cx.has_global::<Supervisor>();
        if installed {
            cx.set_global(Supervisor::default());
        }
        Supervisor::set_health(cx, Health::Running(status), None);
        cx.notify();
        (saved, health, installed)
    })?;
    // Scroll the relays section to the top of the body.
    AnyWindowHandle::from(settings).update(cx, |root, window, cx| -> Result<()> {
        window.draw(cx).clear(cx);
        let at = cx
            .default_global::<RelaysAt>()
            .0
            .context("the relays section did not paint")?;
        let view = root
            .downcast::<SettingsWindow>()
            .map_err(|_| anyhow::anyhow!("unexpected Settings root"))?;
        view.update(cx, |view, cx| {
            let body = view.body_scroll.bounds();
            let offset = view.body_scroll.offset();
            let y = offset.y - (at.top() - body.top());
            view.body_scroll
                .set_offset(point(px(0.), y.max(-view.body_scroll.max_offset().y)));
            cx.notify();
        });
        Ok(())
    })??;
    // Past the details' pop-in, so the shot shows them settled.
    cx.background_executor()
        .timer(crate::motion::ENTER * 3)
        .await;
    AnyWindowHandle::from(settings).update(cx, |_, window, cx| window.draw(cx).clear(cx))?;
    let extension = if wide {
        "relays.png"
    } else {
        "relays-narrow.png"
    };
    let captured = capture_settings(settings, extension, cx).await;
    settings.update(cx, |view, _, cx| {
        view.config.remote.relays = saved;
        if installed {
            cx.remove_global::<Supervisor>();
        } else {
            Supervisor::set_health(cx, health, None);
        }
        view.body_scroll.set_offset(Point::default());
        cx.notify();
    })?;
    captured
}
