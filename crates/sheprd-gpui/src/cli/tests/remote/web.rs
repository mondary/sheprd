use super::*;

#[test]
fn web_reports_status_by_default_and_names_a_relay_with_relay() {
    for args in [&["remote", "web"][..], &["remote", "web", "status"]] {
        assert_eq!(
            command(args),
            ControlCommand::Remote(RemoteCommand::WebStatus)
        );
    }
    assert_eq!(
        command(&["remote", "web", "enable"]),
        ControlCommand::Remote(RemoteCommand::WebEnable { relay: None })
    );
    let invocation = parse(&[
        "remote", "web", "disable", "--relay", "home", "--json", "--window", "2",
    ])
    .unwrap();
    assert_eq!(
        invocation.command,
        ControlCommand::Remote(RemoteCommand::WebDisable {
            relay: Some("home".into())
        })
    );
    assert!(invocation.json);
    assert_eq!(invocation.window, Some(2));
}

#[test]
fn web_refuses_what_it_does_not_do() {
    assert!(matches!(
        parse(&["remote", "web", "approve"]),
        Err(CliError::UnknownCommand { .. })
    ));
    assert!(matches!(
        parse(&["remote", "web", "enable", "--relay"]),
        Err(CliError::MissingValue("--relay"))
    ));
    assert!(parse(&["remote", "web", "enable", "home"]).is_err());
    assert!(parse(&["remote", "web", "status", "--relay", "home"]).is_err());
    assert!(matches!(
        command(&["remote", "web", "enable", "--help"]),
        ControlCommand::Help(Some("remote"))
    ));
}
