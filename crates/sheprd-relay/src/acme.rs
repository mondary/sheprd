//! Certificates for computers' web names (`docs/relay-web-plan.md`): a
//! computer with web access on sends a CSR for `<label>.<relay host>`; the
//! relay checks it, orders the certificate from its ACME CA (Let's Encrypt
//! by default) with its own account, answers the TLS-ALPN-01 challenge at
//! its front, and returns the chain. The computer's private key never
//! leaves it, so the relay still cannot end its browsers' TLS.
//!
//! The account's credentials are the relay's one lasting secret beside the
//! dashboard token, kept in `--acme-dir`. Its URI is logged once ready, so
//! the operator can pin it in the zone's CAA records.

mod challenge;
pub mod csr;
mod error;
pub mod ration;

pub use challenge::{Answer, Challenges, certificate};
pub use error::{Csr, Error};

use crate::{RouteId, WebLabel};
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, NewAccount,
    NewOrder, OrderStatus, RetryPolicy,
};
use ration::Ration;
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{sync::Semaphore, time::timeout};

/// Let's Encrypt's production directory.
pub const LETS_ENCRYPT: &str = "https://acme-v02.api.letsencrypt.org/directory";
/// New certificates a relay orders a week, by default: under Let's
/// Encrypt's 50 per registered domain, with room for the relay's own.
pub const DEFAULT_WEEKLY: usize = 40;
/// How long one order may take, validation and all.
const ORDER_TIMEOUT: Duration = Duration::from_secs(3 * 60);
/// Orders running at once.
const MAX_ORDERS: usize = 4;
/// The file the account's credentials are kept in, inside `--acme-dir`.
const ACCOUNT_FILE: &str = "account.json";

/// Where and how the relay orders certificates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settings {
    /// The ACME directory URL.
    pub directory: String,
    /// Where the account's credentials are kept.
    pub dir: PathBuf,
    /// A CA certificate to trust for the ACME server, for a test CA such
    /// as Pebble; the system's roots otherwise.
    pub root: Option<PathBuf>,
    /// New certificates a week.
    pub weekly: usize,
}

/// Orders computers' certificates and answers their challenges.
pub struct Issuer {
    account: tokio::sync::Mutex<Option<Account>>,
    pub challenges: Arc<Challenges>,
    orders: Semaphore,
    ration: Mutex<Ration>,
    relay_host: Arc<str>,
    settings: Settings,
}

/// The account holds a private key; only the settings are shown.
impl fmt::Debug for Issuer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Issuer")
            .field("relay_host", &self.relay_host)
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl Issuer {
    /// Orders under `settings` for web names under `relay_host`.
    #[must_use]
    pub fn new(settings: Settings, relay_host: Arc<str>) -> Self {
        Self {
            account: tokio::sync::Mutex::new(None),
            challenges: Arc::default(),
            orders: Semaphore::new(MAX_ORDERS),
            ration: Mutex::new(Ration::new(settings.weekly)),
            relay_host,
            settings,
        }
    }

    fn ration(&self) -> std::sync::MutexGuard<'_, Ration> {
        match self.ration.lock() {
            Ok(ration) => ration,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Checks `csr` against `route`'s web name, then orders its
    /// certificate, returning the PEM chain.
    pub async fn issue(&self, route: RouteId, csr: &[u8]) -> Result<String, Error> {
        let label = WebLabel::of(&route);
        let host = label.host(&self.relay_host);
        csr::check(csr, &host)?;
        let _order = self.orders.try_acquire().map_err(|_| Error::Busy)?;
        self.ration().attempt(route, Instant::now())?;
        let chain = timeout(ORDER_TIMEOUT, self.order(label, &host, csr))
            .await
            .map_err(|_| Error::TimedOut)??;
        self.ration().issued(route, Instant::now());
        Ok(chain)
    }

    async fn order(&self, label: WebLabel, host: &str, csr: &[u8]) -> Result<String, Error> {
        let account = self.account().await?;
        let identifiers = [Identifier::Dns(host.to_owned())];
        let mut order = account.new_order(&NewOrder::new(&identifiers)).await?;
        // Held until the order is decided: the validation server may
        // connect more than once, from several places.
        let mut answering = Vec::new();
        let mut authorizations = order.authorizations();
        while let Some(authorization) = authorizations.next().await {
            let mut authorization = authorization?;
            match authorization.status {
                AuthorizationStatus::Valid => continue,
                AuthorizationStatus::Pending => {}
                _ => return Err(Error::Authorization("not pending")),
            }
            let mut challenge = authorization
                .challenge(ChallengeType::TlsAlpn01)
                .ok_or(Error::NoChallenge)?;
            let digest = challenge.key_authorization().digest();
            answering.push(self.challenges.answer(label, host, digest.as_ref())?);
            challenge.set_ready().await?;
        }
        match order.poll_ready(&RetryPolicy::default()).await? {
            OrderStatus::Ready => {}
            OrderStatus::Invalid => return Err(Error::NotReady("invalid")),
            _ => return Err(Error::NotReady("pending")),
        }
        order.finalize_csr(csr).await?;
        let chain = order.poll_certificate(&RetryPolicy::default()).await?;
        drop(answering);
        Ok(chain)
    }

    /// The account, loaded from `--acme-dir` or created there.
    async fn account(&self) -> Result<Account, Error> {
        let mut held = self.account.lock().await;
        if let Some(account) = held.as_ref() {
            return Ok(account.clone());
        }
        let path = self.settings.dir.join(ACCOUNT_FILE);
        let builder = match &self.settings.root {
            Some(root) => Account::builder_with_root(root)?,
            None => Account::builder()?,
        };
        let account = match read(&path).await? {
            Some(credentials) => builder.from_credentials(credentials).await?,
            None => {
                let new = NewAccount {
                    contact: &[],
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                };
                let (account, credentials) = builder
                    .create(&new, self.settings.directory.clone(), None)
                    .await?;
                write(&path, &credentials).await?;
                account
            }
        };
        tracing::info!(
            account = account.id(),
            "the ACME account is ready; pin it in CAA with accounturi"
        );
        *held = Some(account.clone());
        Ok(account)
    }
}

/// The saved credentials, if there are any. File work runs off the
/// runtime's threads.
async fn read(path: &Path) -> Result<Option<AccountCredentials>, Error> {
    let owned = path.to_owned();
    let read = tokio::task::spawn_blocking(move || std::fs::read(owned))
        .await
        .unwrap_or_else(|_| Err(std::io::Error::other("the read was cancelled")));
    match read {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(Error::Saved),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Read {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Saves `credentials` readable by the relay alone, never over a file
/// already there.
async fn write(path: &Path, credentials: &AccountCredentials) -> Result<(), Error> {
    let bytes = serde_json::to_vec(credentials).map_err(Error::Saved)?;
    let owned = path.to_owned();
    let written = tokio::task::spawn_blocking(move || {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        std::io::Write::write_all(&mut options.open(owned)?, &bytes)
    })
    .await
    .unwrap_or_else(|_| Err(std::io::Error::other("the write was cancelled")));
    written.map_err(|source| Error::Write {
        path: path.to_owned(),
        source,
    })
}
