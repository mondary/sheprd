//! Command-line options, parsed from OS strings before any runtime starts.

use crate::{app::config::ActivityDetail, push::Privacy, relay_label::RelayLabel};
use std::{
    ffi::OsString,
    net::{IpAddr, Ipv6Addr},
    path::PathBuf,
    time::Duration,
};

pub const DEFAULT_PORT: u16 = 7443;
const DEFAULT_IDLE_DAYS: u64 = 30;
const MAX_IDLE_DAYS: u64 = 3650;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Certificate {
    /// The daemon's own certificate authority.
    Own,
    /// A certificate chain and key the user provides, as PEM files.
    Files { cert: PathBuf, key: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    /// Where to listen: every interface (`[::]`, dual stack) or one address.
    pub bind: IpAddr,
    /// Whether to advertise over Bonjour, for phones nearby to find it.
    pub bonjour: bool,
    pub certificate: Certificate,
    /// The folder chat sockets live in, as Sheprd resolved it; `None`
    /// resolves it from this process's environment.
    pub chat_dir: Option<PathBuf>,
    /// The desktop endpoint, when explicitly passed by its service plan.
    pub core_endpoint: crate::core_target::CoreTarget,
    /// The Certificate Transparency search web names on relays are looked
    /// up at, instead of Cert Spotter's.
    pub ct_api: Option<url::Url>,
    /// How long a device may stay away before it is forgotten; `None`
    /// keeps it until revoked.
    pub device_idle: Option<Duration>,
    /// Explicit permission to read this machine's GitHub CLI credential.
    pub github_cli: bool,
    /// GitHub Enterprise Server hosts besides those `gh` is signed in to,
    /// whose remotes a phone is told are on GitHub.
    pub github_hosts: Vec<String>,
    /// The Herdr daemon's client socket, as Sheprd resolved it; `None`
    /// discovers it from this process's environment. A service manager's
    /// environment may lack the shell's `XDG_CONFIG_HOME`.
    pub herdr_socket: Option<PathBuf>,
    pub live_activities: ActivityDetail,
    pub port: u16,
    pub privacy: Privacy,
    /// The relays to listen on, so paired phones reach this computer from
    /// any network; none when empty.
    pub relays: Vec<Relay>,
    pub state_dir: PathBuf,
    /// The saved WSL distributions, as Sheprd resolved them; `None` resolves
    /// them from this process's environment.
    pub wsl_catalog: Option<PathBuf>,
}

/// A relay to listen on, what the user calls it, and, for a private
/// relay, where its listen token is and whether browsers reach this
/// computer through it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Relay {
    pub endpoint: sheprd_relay::Endpoint,
    pub label: RelayLabel,
    pub token_file: Option<PathBuf>,
    pub web: bool,
}

impl From<sheprd_relay::Endpoint> for Relay {
    fn from(endpoint: sheprd_relay::Endpoint) -> Self {
        Self {
            endpoint,
            label: RelayLabel::default(),
            token_file: None,
            web: false,
        }
    }
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum ParseError {
    // keep-sorted start
    #[error("--cert and --key go together")]
    CertificatePair,
    #[error("--core-socket and --core-session go together")]
    CorePair,
    #[error("{0} needs a value")]
    Missing(&'static str),
    #[error("{option} names no relay: put it after a --relay")]
    NoRelay { option: &'static str },
    #[error("--state-dir is required")]
    NoStateDir,
    #[error("--relay {relay} has a label it cannot use")]
    RelayLabel {
        relay: String,
        #[source]
        source: crate::relay_label::Error,
    },
    #[error("unknown option {0}")]
    Unknown(String),
    #[error("{option} has an invalid value")]
    Value { option: &'static str },
    // keep-sorted end
}

/// What the command line asks for.
#[derive(Debug, Eq, PartialEq)]
pub enum Invocation {
    Serve(Box<Options>),
    /// Adds a phone another computer's Sheprd allowed, read as JSON from
    /// standard input (`app::enroll`).
    Enroll {
        state_dir: Option<PathBuf>,
    },
    /// Keeps a phone's relay keys another computer's Sheprd carried here,
    /// read as JSON from standard input (`app::relay_enroll`).
    RelayEnroll {
        state_dir: Option<PathBuf>,
    },
    /// Revokes a device that other computer enrolled.
    Revoke {
        state_dir: Option<PathBuf>,
        device: String,
    },
    /// Lists the relays this computer listens on, taking any another
    /// computer shares on standard input (`control::relays::Offer`).
    Relays {
        state_dir: Option<PathBuf>,
        offer: bool,
    },
    /// Reads this computer's text shortcuts, offering any given on standard
    /// input.
    Shortcuts {
        state_dir: Option<PathBuf>,
        offer: bool,
    },
    /// Reads this computer's tags, offering any given on standard input.
    Tags {
        state_dir: Option<PathBuf>,
        offer: bool,
    },
    Help,
    Version,
}

pub const USAGE: &str = "\
Usage: sheprd-remote-control serve --state-dir DIR [options]
       sheprd-remote-control enroll [--state-dir DIR] < REQUEST.json
       sheprd-remote-control revoke --device ID [--state-dir DIR]
       sheprd-remote-control relay-enroll [--state-dir DIR] < REQUEST.json
       sheprd-remote-control tags [--offer] [--state-dir DIR] [< TAGS.json]
       sheprd-remote-control shortcuts [--offer] [--state-dir DIR] [< SHORTCUTS.json]
       sheprd-remote-control relays [--offer] [--state-dir DIR] [< RELAYS.json]

Serves this computer's Herdr chats to paired phones over HTTPS. Sheprd
starts it when Remote control is on.

Options:
  --state-dir DIR          Where keys, devices, and certificates live (required)
  --port N                 HTTPS port (default 7443)
  --bind all|ADDRESS       Listen on every interface (default) or one address
  --cert FILE --key FILE   Use this certificate chain and key instead of the
                           daemon's own certificate authority
  --herdr-socket PATH      The Herdr daemon's client socket (default: discovered);
                           other running Herdr sessions are followed too
  --core-socket PATH --core-session NAME
                           The desktop core endpoint (default: discovered)
  --chat-dir DIR           Where chat sockets live (default: discovered)
  --github-cli             Read PRs/checks/logs using this host's gh sign-in;
                           permits paired phones to read accessible repositories
  --github-host HOST       Count HOST as a GitHub Enterprise Server host for
                           phones, besides those gh is signed in to; repeatable
  --wsl-catalog FILE       The saved WSL distributions (default: discovered)
  --privacy generic|excerpt
                           Whether notifications may quote the chat (default excerpt)
  --live-activities detailed|generic
                           Whether Live Activities name the chat and its step
                           (default detailed)
  --device-idle-days N     Forget devices unseen this long, 0 for never
                           (default 30)
  --relay URL              Listen on this relay too, so paired phones reach
                           this computer from any network; repeatable
  --relay-name NAME        What the --relay before it is called, such as fra3
  --relay-description TEXT A line about the --relay before it, such as
                           not for public use
  --relay-tag TAG          A tag for the --relay before it; repeatable
  --relay-token-file FILE  The --relay before it is private: present the
                           listen token in FILE
  --relay-web              Let browsers reach this computer at its web name on
                           the --relay before it (a private relay's)
  --ct-api URL             The Certificate Transparency search to watch web
                           names with (default Cert Spotter's issuances API)
  --no-bonjour             Do not advertise over Bonjour; phones then pair
                           by QR code only (on macOS, Sheprd advertises
                           while its pairing dialog is open instead)
  -h, --help               Show this help
  -V, --version            Show the version

enroll and revoke are what Sheprd on another computer runs here over SSH,
so a phone paired there also reaches this computer. enroll starts the
daemon when it does not run and prints this computer's answer as JSON.
relay-enroll keeps a phone's relay keys, signed with its own key here, so
the phone reaches this computer through its relays, and prints the outcome
as JSON; it needs the daemon running.
relays --offer takes {\"source\", \"relays\"}, the relays Sheprd on
another computer listens on, replacing what that computer shared before:
the daemon listens on them too, beside its own [remote] relays.
Their state directory defaults to the one Sheprd on this computer uses.
";

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, ParseError> {
    let mut args = args.into_iter().skip(1);
    match args.next().as_deref().and_then(|arg| arg.to_str()) {
        Some("serve") => {}
        Some(verb @ ("enroll" | "revoke")) => return parse_host(verb == "enroll", args),
        // Takes `enroll`'s options, `--state-dir` alone.
        Some("relay-enroll") => {
            return parse_host(true, args).map(|invocation| match invocation {
                Invocation::Enroll { state_dir } => Invocation::RelayEnroll { state_dir },
                other => other,
            });
        }
        Some("relays") => {
            return parse_edited(args, |state_dir, offer| Invocation::Relays {
                state_dir,
                offer,
            });
        }
        Some("shortcuts") => {
            return parse_edited(args, |state_dir, offer| Invocation::Shortcuts {
                state_dir,
                offer,
            });
        }
        Some("tags") => {
            return parse_edited(args, |state_dir, offer| Invocation::Tags {
                state_dir,
                offer,
            });
        }
        Some("-h" | "--help") | None => return Ok(Invocation::Help),
        Some("-V" | "--version") => return Ok(Invocation::Version),
        Some(other) => return Err(ParseError::Unknown(other.to_owned())),
    }
    let mut state_dir = None;
    let mut port = DEFAULT_PORT;
    let mut bind = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
    let (mut cert, mut key) = (None, None);
    let mut privacy = Privacy::default();
    let mut live_activities = ActivityDetail::default();
    let mut idle_days = DEFAULT_IDLE_DAYS;
    let (mut herdr_socket, mut chat_dir, mut wsl_catalog) = (None, None, None);
    let (mut core_socket, mut core_session) = (None, None);
    let mut bonjour = true;
    let mut github_cli = false;
    let mut github_hosts = Vec::new();
    let mut relays = Vec::new();
    let mut ct_api = None;
    while let Some(arg) = args.next() {
        let name = arg.to_str().unwrap_or_default().to_owned();
        let mut value = |option: &'static str| args.next().ok_or(ParseError::Missing(option));
        let text = |value: OsString, option: &'static str| {
            value
                .into_string()
                .map_err(|_| ParseError::Value { option })
        };
        match name.as_str() {
            "--state-dir" => state_dir = Some(PathBuf::from(value("--state-dir")?)),
            "--port" => {
                port = text(value("--port")?, "--port")?
                    .parse()
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or(ParseError::Value { option: "--port" })?;
            }
            "--bind" => {
                let address = text(value("--bind")?, "--bind")?;
                if address != "all" {
                    bind = address
                        .parse()
                        .map_err(|_| ParseError::Value { option: "--bind" })?;
                }
            }
            "--cert" => cert = Some(PathBuf::from(value("--cert")?)),
            "--key" => key = Some(PathBuf::from(value("--key")?)),
            "--herdr-socket" => herdr_socket = Some(PathBuf::from(value("--herdr-socket")?)),
            "--core-socket" => core_socket = Some(PathBuf::from(value("--core-socket")?)),
            "--core-session" => {
                core_session = Some(text(value("--core-session")?, "--core-session")?)
            }
            "--chat-dir" => chat_dir = Some(PathBuf::from(value("--chat-dir")?)),
            "--wsl-catalog" => wsl_catalog = Some(PathBuf::from(value("--wsl-catalog")?)),
            "--privacy" => {
                privacy = match text(value("--privacy")?, "--privacy")?.as_str() {
                    "generic" => Privacy::Generic,
                    "excerpt" => Privacy::Excerpt,
                    _ => {
                        return Err(ParseError::Value {
                            option: "--privacy",
                        });
                    }
                };
            }
            "--live-activities" => {
                live_activities =
                    match text(value("--live-activities")?, "--live-activities")?.as_str() {
                        "detailed" => ActivityDetail::Detailed,
                        "generic" => ActivityDetail::Generic,
                        _ => {
                            return Err(ParseError::Value {
                                option: "--live-activities",
                            });
                        }
                    };
            }
            "--device-idle-days" => {
                idle_days = text(value("--device-idle-days")?, "--device-idle-days")?
                    .parse()
                    .ok()
                    .filter(|days| *days <= MAX_IDLE_DAYS)
                    .ok_or(ParseError::Value {
                        option: "--device-idle-days",
                    })?;
            }
            "--relay" => {
                let endpoint = text(value("--relay")?, "--relay")?;
                relays.push(
                    sheprd_relay::Endpoint::parse(&endpoint)
                        .map_err(|_| ParseError::Value { option: "--relay" })?
                        .into(),
                );
            }
            "--relay-name" => {
                let name = text(value("--relay-name")?, "--relay-name")?;
                last_label(&mut relays, "--relay-name")?.name = Some(name);
            }
            "--relay-description" => {
                let description = text(value("--relay-description")?, "--relay-description")?;
                last_label(&mut relays, "--relay-description")?.description = Some(description);
            }
            "--relay-tag" => {
                let tag = text(value("--relay-tag")?, "--relay-tag")?;
                last_label(&mut relays, "--relay-tag")?.tags.push(tag);
            }
            "--relay-token-file" => {
                let path = PathBuf::from(value("--relay-token-file")?);
                last_relay(&mut relays, "--relay-token-file")?.token_file = Some(path);
            }
            "--relay-web" => last_relay(&mut relays, "--relay-web")?.web = true,
            "--ct-api" => {
                let text = text(value("--ct-api")?, "--ct-api")?;
                ct_api = Some(
                    crate::relay::web::transparency::api(&text)
                        .map_err(|_| ParseError::Value { option: "--ct-api" })?,
                );
            }
            "--no-bonjour" => bonjour = false,
            "--github-cli" => github_cli = true,
            "--github-host" => {
                let host = text(value("--github-host")?, "--github-host")?;
                if !crate::control::github::valid_host(&host) {
                    return Err(ParseError::Value {
                        option: "--github-host",
                    });
                }
                github_hosts.push(host);
            }
            "-h" | "--help" => return Ok(Invocation::Help),
            _ => return Err(ParseError::Unknown(name)),
        }
    }
    for relay in &relays {
        relay
            .label
            .check()
            .map_err(|source| ParseError::RelayLabel {
                relay: relay.endpoint.to_string(),
                source,
            })?;
    }
    let certificate = match (cert, key) {
        (None, None) => Certificate::Own,
        (Some(cert), Some(key)) => Certificate::Files { cert, key },
        _ => return Err(ParseError::CertificatePair),
    };
    let core_endpoint = match (core_socket, core_session) {
        (None, None) => crate::core_target::CoreTarget::Discover,
        (Some(socket), Some(session)) => {
            crate::core_target::CoreTarget::Endpoint(sheprd_core_client::Endpoint {
                socket,
                session,
            })
        }
        _ => return Err(ParseError::CorePair),
    };
    Ok(Invocation::Serve(Box::new(Options {
        bind,
        bonjour,
        certificate,
        chat_dir,
        ct_api,
        core_endpoint,
        device_idle: (idle_days != 0).then(|| Duration::from_secs(idle_days * 24 * 60 * 60)),
        github_cli,
        github_hosts,
        herdr_socket,
        live_activities,
        port,
        privacy,
        relays,
        state_dir: state_dir.ok_or(ParseError::NoStateDir)?,
        wsl_catalog,
    })))
}

/// The label of the `--relay` that `option` follows.
fn last_label<'a>(
    relays: &'a mut [Relay],
    option: &'static str,
) -> Result<&'a mut RelayLabel, ParseError> {
    last_relay(relays, option).map(|relay| &mut relay.label)
}

/// The relay the option follows.
fn last_relay<'a>(
    relays: &'a mut [Relay],
    option: &'static str,
) -> Result<&'a mut Relay, ParseError> {
    relays.last_mut().ok_or(ParseError::NoRelay { option })
}

/// `enroll [--state-dir DIR]` and `revoke --device ID [--state-dir DIR]`.
fn parse_host(
    enroll: bool,
    mut args: impl Iterator<Item = OsString>,
) -> Result<Invocation, ParseError> {
    let (mut state_dir, mut device) = (None, None);
    while let Some(arg) = args.next() {
        let name = arg.to_str().unwrap_or_default().to_owned();
        match name.as_str() {
            "--state-dir" => {
                state_dir = Some(PathBuf::from(
                    args.next().ok_or(ParseError::Missing("--state-dir"))?,
                ));
            }
            "--device" if !enroll => {
                device = Some(
                    args.next()
                        .ok_or(ParseError::Missing("--device"))?
                        .into_string()
                        .ok()
                        .filter(|device| !device.is_empty())
                        .ok_or(ParseError::Value { option: "--device" })?,
                );
            }
            "-h" | "--help" => return Ok(Invocation::Help),
            _ => return Err(ParseError::Unknown(name)),
        }
    }
    if enroll {
        return Ok(Invocation::Enroll { state_dir });
    }
    Ok(Invocation::Revoke {
        state_dir,
        device: device.ok_or(ParseError::Missing("--device"))?,
    })
}

/// `tags`, `shortcuts`, or `relays`, then `[--offer] [--state-dir DIR]`, as `made`.
fn parse_edited(
    mut args: impl Iterator<Item = OsString>,
    made: fn(Option<PathBuf>, bool) -> Invocation,
) -> Result<Invocation, ParseError> {
    let (mut state_dir, mut offer) = (None, false);
    while let Some(arg) = args.next() {
        let name = arg.to_str().unwrap_or_default().to_owned();
        match name.as_str() {
            "--state-dir" => {
                state_dir = Some(PathBuf::from(
                    args.next().ok_or(ParseError::Missing("--state-dir"))?,
                ));
            }
            "--offer" => offer = true,
            "-h" | "--help" => return Ok(Invocation::Help),
            _ => return Err(ParseError::Unknown(name)),
        }
    }
    Ok(made(state_dir, offer))
}

/// The computer's name as its owner knows it.
pub(crate) fn computer_name() -> String {
    #[cfg(unix)]
    {
        let name = rustix::system::uname()
            .nodename()
            .to_string_lossy()
            .into_owned();
        let name = name.strip_suffix(".local").unwrap_or(&name).to_owned();
        if !name.is_empty() {
            return name;
        }
    }
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "This computer".to_owned())
}
