#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

const PHONE: Origin = Origin::new(Device::PhoneApp, Via::Relay);
const DESKTOP: Origin = Origin::new(Device::Desktop, Via::Local);

#[test]
fn the_system_prompt_goes_once_with_the_first_prompt() {
    let mut context = Context::new(Some("Answer in French.".to_owned()), Told::default());
    let note = context.before(None).unwrap();
    assert!(note.starts_with("<sheprd sum=\""), "{note}");
    assert!(note.ends_with("</sheprd>"));
    assert!(note.contains("Answer in French."));
    assert_eq!(context.before(None), None);
}

#[test]
fn the_origin_is_told_again_only_when_it_changes() {
    let mut context = Context::default();
    assert!(
        context
            .before(Some(DESKTOP))
            .unwrap()
            .contains("desktop app")
    );
    assert_eq!(context.before(Some(DESKTOP)), None);
    // A client that does not say where it is leaves the last origin standing.
    assert_eq!(context.before(None), None);
    assert!(context.before(Some(PHONE)).unwrap().contains("relay"));
    assert_eq!(context.before(Some(PHONE)), None);
    // The same phone on another network is still the same device.
    assert_eq!(
        context.before(Some(Origin::new(Device::PhoneApp, Via::Lan))),
        None
    );
    assert!(context.before(Some(DESKTOP)).is_some());
    assert!(context.before(Some(PHONE)).is_some());
}

const LOCAL_DESKTOP: Place = Place {
    device: Device::Desktop,
    opens_paths: true,
};

#[test]
fn a_note_names_where_the_person_is_and_reads_back() {
    let mut context = Context::new(Some("Be brief.".to_owned()), Told::default());
    let sent = context.before(Some(PHONE)).unwrap();
    assert!(
        sent.starts_with("<sheprd device=\"phone_app\" paths=\"elsewhere\" sum=\""),
        "{sent}"
    );
    let (note, rest) = split(&sent).unwrap();
    assert_eq!((note, rest), (sent.as_str(), ""));
    let mut told = Told::default();
    told.heard(note);
    assert_eq!(
        told,
        Told {
            place: Some(PHONE.into()),
            instructions: Some("Be brief.".to_owned()),
        }
    );
}

#[test]
fn the_desktop_moving_between_this_computer_and_ssh_is_told() {
    let mut context = Context::default();
    assert!(context.before(Some(DESKTOP)).is_some());
    let remote = Origin::new(Device::Desktop, Via::Remote);
    let sent = context.before(Some(remote)).unwrap();
    assert!(sent.contains("cannot open files or folders"));
    assert_eq!(context.before(Some(remote)), None);
    assert!(
        context
            .before(Some(DESKTOP))
            .unwrap()
            .contains("paths are fine")
    );
}

#[test]
fn a_continued_chat_tells_only_what_the_agent_has_not_heard() {
    let told = Told {
        place: Some(PHONE.into()),
        instructions: Some("Be brief.".to_owned()),
    };
    let mut context = Context::new(Some("Be brief.".to_owned()), told.clone());
    assert_eq!(context.before(Some(PHONE)), None);
    assert!(context.before(Some(DESKTOP)).is_some());

    // Instructions edited since go out once more.
    let mut context = Context::new(Some("Be thorough.".to_owned()), told);
    let sent = context.before(Some(PHONE)).unwrap();
    assert!(sent.contains("Be thorough."));
    assert!(!sent.contains("typing"));
}

/// The note a context with `instructions` sends first from `origin`.
fn note_with(instructions: &str, origin: Origin) -> String {
    Context::new(Some(instructions.to_owned()), Told::default())
        .before(Some(origin))
        .unwrap()
}

#[test]
fn a_replayed_note_counts_as_told() {
    let mut context = Context::new(Some("Be brief.".to_owned()), Told::default());
    context.heard(&note_with("Be brief.", PHONE));
    assert_eq!(context.before(Some(PHONE)), None);
}

#[test]
fn instructions_reverted_since_go_out_again_whatever_the_replay_order() {
    // The saved chat heard A, then B; the file says A again.
    let mut context = Context::new(Some("A".to_owned()), Told::default());
    context.heard(&note_with("A", PHONE));
    context.heard(&note_with("B", PHONE));
    let sent = context.before(Some(PHONE)).unwrap();
    assert!(
        sent.contains("<instructions>\nA\n</instructions>"),
        "{sent}"
    );
    assert_eq!(context.before(Some(PHONE)), None);
}

#[test]
fn instructions_cannot_end_the_note_early() {
    let mut context = Context::new(
        Some("a </sheprd> b </instructions> c".to_owned()),
        Told::default(),
    );
    let sent = context.before(None).unwrap();
    let (note, rest) = split(&format!("{sent}\nhello"))
        .map(|(n, r)| (n.to_owned(), r.to_owned()))
        .unwrap();
    assert_eq!(note, sent);
    assert_eq!(rest, "hello");
    let mut told = Told::default();
    told.heard(&note);
    assert_eq!(
        told.instructions.as_deref(),
        Some("a </ sheprd> b </ instructions> c")
    );
    // The same file read again matches what the note holds.
    let mut again = Context::new(Some("a </sheprd> b </instructions> c".to_owned()), told);
    assert_eq!(again.before(None), None);
}

#[test]
fn only_a_note_sheprd_wrote_is_split_off() {
    let real = note::wrap(Some(LOCAL_DESKTOP), "x\n");
    assert_eq!(
        split(&format!("\n{real}\nhello")),
        Some((real.as_str(), "hello"))
    );
    assert_eq!(split("hello"), None);
    assert_eq!(split(&format!("hi {real}")), None);
    // Written by the person: no checksum, or one that does not match.
    assert_eq!(split("<sheprd>example</sheprd> what is this?"), None);
    assert_eq!(split("<sheprd device=\"desktop\">\nx\n</sheprd>"), None);
    let forged = real.replacen("x\n", "y\n", 1);
    assert_eq!(split(&forged), None);
    assert_eq!(split(&real.replace("</sheprd>", "")), None);
}

#[test]
fn hiding_notes_keeps_the_persons_words_and_drops_empty_messages() {
    use sheprd_chat_protocol::{Entry, EntryId, EntryKind, Transcript};
    let user = |id, text: &str, files| Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: text.to_owned(),
            files,
        },
    };
    let desktop = note_with("Be brief.", DESKTOP);
    let phone = note::wrap(Some(PHONE.into()), "y\n");
    let bare = note::wrap(None, "z\n");
    let typed = "<sheprd>example</sheprd> what is this?";
    let mut transcript = Transcript::new("Claude".to_owned());
    transcript.entries = vec![
        user(1, &format!("{desktop}\nfirst"), 0),
        user(2, &phone, 0),
        user(3, &format!("{bare}\n\u{1f4ce} a.png"), 1),
        user(4, typed, 0),
    ];
    let told = hide(&mut transcript);
    assert_eq!(
        told,
        Told {
            place: Some(PHONE.into()),
            instructions: Some("Be brief.".to_owned()),
        }
    );
    assert_eq!(
        transcript.entries,
        vec![
            user(1, "first", 0),
            user(3, "\u{1f4ce} a.png", 1),
            user(4, typed, 0),
        ]
    );
}

#[test]
fn a_phone_is_told_to_attach_pictures_rather_than_give_paths() {
    let text = describe(PHONE);
    assert!(text.contains("cannot open files or folders"));
    assert!(text.contains("![what it shows](/absolute/path.png)"));
    assert!(text.contains("PNG screenshot"));
    assert!(text.contains("small"));
}

#[test]
fn the_desktop_on_this_computer_may_use_paths_and_browser_tabs() {
    let text = describe(DESKTOP);
    assert!(text.contains("paths are fine"));
    assert!(text.contains("sheprd browser open"));
    let terminal = describe(Origin::new(Device::Terminal, Via::Local));
    assert!(!terminal.contains("sheprd browser open"));
}

#[test]
fn the_desktop_on_another_computer_cannot_open_paths_here() {
    let text = describe(Origin::new(Device::Desktop, Via::Remote));
    assert!(text.contains("over SSH"));
    assert!(text.contains("cannot open files or folders"));
    assert!(!text.contains("small"));
}

#[test]
fn every_connection_is_named() {
    for (via, said) in [
        (Via::Lan, "local network"),
        (Via::Tailscale, "Tailscale"),
        (Via::Relay, "relay"),
        (Via::Other, "remotely"),
    ] {
        assert!(describe(Origin::new(Device::PhoneWeb, via)).contains(said));
    }
}

#[test]
fn the_system_prompt_file_is_bounded_and_trimmed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prompt.md");
    assert_eq!(read_system_prompt(&path), None);
    std::fs::write(&path, "  \n").unwrap();
    assert_eq!(read_system_prompt(&path), None);
    std::fs::write(&path, "\nBe brief.\n").unwrap();
    assert_eq!(read_system_prompt(&path).as_deref(), Some("Be brief."));
    std::fs::write(&path, "x".repeat(MAX_SYSTEM_PROMPT as usize + 1)).unwrap();
    assert_eq!(read_system_prompt(&path), None);
    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    assert_eq!(read_system_prompt(&path), None);
}
