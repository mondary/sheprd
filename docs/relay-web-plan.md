# Plan: the web app through a relay

Today the phone web app (the PWA `sheprd-remote-control` serves) works only
on the computer's network or over Tailscale, and only after the phone
installs the daemon's own certificate authority. The iOS app reaches a
computer from anywhere through `sheprd-relay` (`docs/relay-plan.md`), but a
browser cannot: it cannot run the Noise handshake inside a WebSocket and
then treat the result as an origin.

This plan lets any browser open `https://<id>.fra1.sheprd-relay.dev`, add it
to the home screen, and control the computer from any network, without the
iOS app and without installing a CA. The relay still sees only ciphertext:
TLS runs from the browser to the computer, and the relay routes it by name
without decrypting it. Read `AGENTS.md` first; its rules apply throughout.

## Decisions

| Question | Decision |
| --- | --- |
| Address | One hostname per computer per relay: `<id>.<relay host>`, never a path. A path can only be read after TLS is decrypted, and every computer under one host would share one origin (storage, service worker, push). |
| Where TLS ends | On the computer. The relay peeks at the ClientHello's SNI and splices the raw bytes to that computer's listener. |
| Certificate | One per computer, never a wildcard: a wildcard would sit on the relay and let it read and change every user's app. The computer makes its key and a CSR; the key never leaves it. |
| Who talks to Let's Encrypt | The relay, for the computer's CSR, over the listener's authenticated connection. Daemons carry no ACME client; CAA can pin issuance to the relay's account. |
| Domain | A dedicated domain, `sheprd-relay.dev`, holding nothing but relays (below). `IP.sslip.io` only for testing. |
| Device sign-in | Pairing as today (QR, dual approval, matching code), plus a **required passkey** per web device. Refreshing a short-lived token needs a passkey assertion. |
| Mis-issued certificates | The daemon watches Certificate Transparency for its own name and turns web access off when a certificate with another key appears. |
| Default | Off. The user turns web access on in Sheprd, per computer, and confirms that it puts the computer's sign-in page on the internet. |

### Still open

1. **Web access on Sheprd's public relays.** Anyone can listen on a public
   relay, so anyone could serve a page of their choosing at a name under
   our domain: free HTTPS hosting, and phishing pages that harm the
   domain's reputation. The relay cannot see or filter that content. The
   recommendation is to ship web access on private relays first, then on
   ours with a per-relay `web = off | token | open` setting, a route deny
   list for abuse reports, per-route bandwidth caps, and an abuse contact.
2. **Public Suffix List entries** for each relay host, once usage justifies
   them (Domain, below).

## Domain

Buy a dedicated domain for relays. `sheprd.org` should keep the website,
downloads, and anything with a login; the relay domain should hold relays
and nothing else.

**The relay domain is `sheprd-relay.dev`** (registered). `.dev` is on the
HSTS preload list as a whole TLD, so browsers never try plain HTTP to any
name under it, even on the first visit.

Why a separate domain is needed for security, not only tidiness:

- **Same-site boundaries.** Browsers treat every name under one registered
  domain as the same site. Each computer's page is served by its owner's
  daemon, and anyone can listen on a public relay, so a page under
  `relay.sheprd.org` could set cookies for `sheprd.org` and weaken
  `SameSite` protections on the website and anything else there. The
  remote control app uses bearer tokens, not cookies, so it is not exposed
  to this, but the rest of `sheprd.org` would be.
- **Reputation.** If a stranger's page under the relay domain is reported to
  Safe Browsing or a corporate filter, the whole registered domain can be
  blocked. A dedicated domain keeps that away from the website, update
  checks, and downloads.
- **Rate limits.** Let's Encrypt allows 50 new certificates per registered
  domain every 7 days, counted across all accounts; renewals through ARI do
  not count. A dedicated domain does not share that budget with the website,
  and can get its own limit increase.
- **CAA and DNSSEC.** A zone that holds only relays can carry strict CAA
  records (below) without affecting anything else.

Why not sslip.io for real use: `sslip.io` is not on the Public Suffix List,
so every sslip.io user in the world shares one budget of 50 new certificates
a week, and sslip.io's DNS sits in the trust path. It stays the zero-config
default for the relay's own name and for testing web access on a private
relay.

### DNS layout

```text
fra1.sheprd-relay.dev       A/AAAA  <fra1 droplet>     the relay itself
*.fra1.sheprd-relay.dev     A/AAAA  <fra1 droplet>     every computer listening on fra1
fra1.sheprd-relay.dev       CAA 0 issue "letsencrypt.org; accounturi=<fra1 relay account>; validationmethods=tls-alpn-01"
fra1.sheprd-relay.dev       CAA 0 issue "letsencrypt.org; accounturi=<fra1 Caddy account>; validationmethods=tls-alpn-01"
sheprd-relay.dev            CAA 0 issue ";"                                         nothing else issues here
```

- **One wildcard record per relay, added when it is deployed**, never per
  computer. A wildcard covers one IP, so each relay gets its own subzone.
  `create-droplet.sh` adds both records through `doctl` when the domain's
  DNS is on DigitalOcean.
- **CAA is inherited down the tree**, so the records on `fra1` also cover
  `<id>.fra1`. Each relay's own ACME accounts are the only ones that can
  issue for its names. A compromised `fra1` cannot get certificates for
  `nyc3` names, and someone who only takes over a relay's IP address,
  without its account key, cannot issue at all.
- **Relays outside DigitalOcean.** The zone lives at one DNS provider
  whatever hosts the relays, and its records point at any IP. A relay can
  run on any host that gives it a public address with TCP and UDP 443
  reaching it unchanged: a VM (Hetzner, Vultr, AWS EC2, OVH) with
  `cloud-init.yml`, or any Linux box with `compose.yml`. A load balancer in
  front must pass TCP through (an AWS NLB in TCP mode, with PROXY protocol
  v2), never end TLS (an AWS ALB, App Platform, or Cloudflare's proxy). On
  Cloudflare DNS, relay records stay "DNS only". Every deployment prints the
  records to add (address, wildcard, CAA); `create-droplet.sh` adds them
  itself only when the zone is on DigitalOcean.
- **Turn on DNSSEC, registrar lock, and two-factor sign-in at the
  registrar.** Whoever controls this zone can issue for every name in it.
- **Ask Let's Encrypt for a rate limit increase** for `sheprd-relay.dev`
  before the first public relay. Their form takes a few weeks.
- **Public Suffix List, later.** The list refuses entries made only to work
  around rate limits, and refuses wildcard entries. Explicit per-relay
  entries (`fra1.sheprd-relay.dev`) are justified the way `ngrok.app` and
  `trycloudflare.com` are, by subdomains that belong to mutually untrusting
  parties. Apply once there are enough users; the list declines small
  projects.

## Architecture

```text
browser ──TCP 443, TLS (SNI <id>.fra1.sheprd-relay.dev)──▶ relay front
                                                            │ peeks SNI, never decrypts
                                                            ▼
computer (sheprd-remote-control) ◀──── web session over the listener (QUIC or WebSocket)
   └── ends TLS with its own certificate for <id>.fra1.sheprd-relay.dev
   └── serves the existing router: shell, service worker, API, SSE, Web Push

computer ──CSR (listen connection)──▶ relay ──ACME──▶ Let's Encrypt
computer ◀──certificate chain────────┘   (answers TLS-ALPN-01 at its own front)
```

The hostname label is `base32(route[..20])`, lowercase, 32 characters: one
DNS label, tied to the route the relay already proves, and different on
each relay because routes are (`docs/relay-plan.md`, What clients must hold
to).

## Relay (`crates/sheprd-relay`, `deploy/relay`)

### The TCP front

The relay takes TCP 443 from Caddy and reads the ClientHello before routing:

- **Bounds:** at most 16 KiB and 5 seconds for the ClientHello; a strict
  parser that rejects anything malformed. Fuzzed, with tests for fragmented
  records, missing SNI, oversized extensions, and slow senders.
- **The relay's own name** (`fra1.sheprd-relay.dev`): the bytes go to Caddy
  on an internal port, prefixed with PROXY protocol v2 so bans still see
  the caller's address. Caddy keeps the dashboard and the WebSocket
  fallback.
- **`<label>.<relay host>` with ALPN `acme-tls/1`**: answered by the relay
  itself while it has a TLS-ALPN-01 challenge pending for that name, and
  closed otherwise. The challenge never reaches the computer.
- **`<label>.<relay host>` for a listener that has web access on**: a new
  web session to that listener (below).
- **Anything else**, including a label whose listener is offline or has web
  access off: the connection is closed, the same answer for each, so a
  scan cannot tell them apart.

The label is a route prefix, so it is never logged. Metrics count web
sessions, refusals, and bytes without names.

### Web sessions

- The listener turns web access on with `{"t":"web","enabled":true}` on its
  control socket (a field in the QUIC listen request), answered with a
  status. Until then, its name does not answer.
- The relay announces each web session with
  `{"t":"incoming","session","kind":"web"}`, with no dial key. The listener
  accepts it as it accepts a dial today (`/v1/accept/{session}` or a QUIC
  stream), and the stream carries the browser's raw TLS bytes.
- Dial-key checks do not apply to web sessions: the computer authenticates
  the browser itself (passkeys, below). That is why web access is opt-in.
- **Limits:** concurrent web sessions per route (16) and in total, new
  connections per caller address per minute, bytes per session and per
  route, and idle and total session time, all with the same knobs and
  `Limits` style as the existing limits.
- **Deny list:** an operator can block a route's web access (an abuse
  report) without a restart; the listener hears a status that says so.

### Certificates for computers

- **Account.** On first start the relay creates an ACME account key (P-256)
  and registers it, keeping the key where it keeps the dashboard token
  (`cloud-init` writes it 0600; App Platform passes it as a secret). The
  relay prints the account URI at start so the operator can put it in CAA.
  This is the relay's only lasting secret besides the dashboard token, and
  losing it costs only re-registration and a CAA update.
- **Request.** The listener sends `{"t":"csr","csr":"<base64 DER>"}`. The
  relay checks:
  - the CSR's signature;
  - exactly one name, and it is this listener's own label under this
    relay's host;
  - the key is P-256 or Ed25519 (if Let's Encrypt accepts it by then);
  - this route has not passed its issuance allowance.
- **Issuance.** The relay orders the certificate with `instant-acme`, answers
  TLS-ALPN-01 at its front, and returns `{"t":"certificate","chain"}` or a
  status (`refused`, `rate_limited`, `failed`). Orders live in memory only
  and are bounded; a restart drops them, and the listener asks again.
- **Renewal.** The listener asks again at two-thirds of the certificate's
  lifetime, with the same key unless it is rotating. The relay checks ARI
  and renews within its window, so renewals do not count against the
  weekly limit. The plan holds as lifetimes shrink.
- **Rationing.** At most one new (non-renewal) certificate per route per
  three days, and a per-relay weekly cap below the domain's limit (default
  40, divided by the number of relays the operator runs). It is in memory,
  so a restart resets it; Let's Encrypt's own limit stays the hard stop.

### Deployment

- `compose.yml`: the relay publishes TCP and UDP 443; Caddy moves to an
  internal port behind PROXY protocol, and keeps getting the relay's own
  certificate.
- `cloud-init.yml`, `create-droplet.sh`: generate the ACME account key, add
  the wildcard record, and print the CAA records to add.
- `app.yaml` (App Platform): no web access. App Platform ends TLS itself, so
  it cannot pass the browser's TLS to a computer; `/v1/info` says
  `"web": false`.
- `/v1/info` gains `"web": bool`, so Sheprd can tell whether a relay offers
  it before the user picks it.

## Daemon (`crates/sheprd-remote-control`)

- **Key and CSR** (`relay/web/certificate.rs`): a P-256 key per relay with web
  access, stored 0600 beside the relay key, a CSR for the computer's label,
  and the returned chain checked before use:
  - the leaf's public key is this computer's own;
  - the only name is the one it asked for;
  - it chains to the web PKI roots (`webpki-roots`), is valid now, and has
    the serverAuth EKU.

  Anything else is discarded and logged as the relay misbehaving.
- **Serving** (`relay/web.rs`): each web session runs a rustls server with that
  certificate, then the existing router through `serve_connection`, as relay
  sessions already do. The TLS resolver chooses by SNI: the relay name gets
  the public certificate; LAN and Tailscale keep the own-CA certificate.
- **Origins**: the relay origin is added as a home origin. Browsers block a
  public origin from reaching private addresses (Private Network Access),
  so an app installed from the relay talks only to the relay, also at home.
- **Pairing on the web origin**: the QR opens
  `https://<id>.fra1.sheprd-relay.dev/pair#…` with the pairing secret in
  the fragment, so no server logs it. No CA profile to install. Dual approval
  and the matching code are unchanged. `/pair` answers only while a QR is
  live and is a plain 404 otherwise.
- **Passkeys, required** (`auth/passkey.rs`, `webauthn-rs`):
  - The relying party ID is the full hostname, not the registered domain, so
    a passkey for one computer never works for another.
  - Pairing on the web origin registers a passkey (user verification
    required) before Allow can complete; the desktop dialog says the device
    will use one.
  - Access tokens last 10 minutes. A refresh needs a fresh assertion, so a
    stolen token expires and a stolen refresh is useless without the
    authenticator.
  - The device record keeps the credential ID and public key; revoking the
    device removes them and ends its sessions.
  - A device paired on the LAN can add a passkey to use the relay origin;
    without one, the relay origin refuses it.
- **Certificate Transparency watch** (`relay/web/transparency.rs`): every
  hour, and after each issuance, the daemon asks a CT search service (Cert
  Spotter's API by default, configurable) for certificates issued for its
  own names. A certificate with a public key that is not one of its own:
  - turns web access off on that relay;
  - shows an alert in Sheprd and pushes one to paired devices;
  - stays off until the user turns it on again, after rotating to a new
    label and key if they choose to.

  The query names a hostname that CT logs already make public. A failed
  query is reported in status, never treated as clean.
- **Hardening on the web origin**: the existing strict CSP (Alpine's CSP
  build), plus HSTS, `Cross-Origin-Opener-Policy: same-origin`,
  `Cross-Origin-Resource-Policy: same-origin`, `Referrer-Policy: no-referrer`,
  and no third-party scripts. Unauthenticated routes (`/api/log`, the
  shell, `/pair`) get per-address limits and are reviewed as being on the
  open internet, because they are.
- **Signed shell** (after the first release): the service worker serves the
  shell from its cache and accepts new assets only under a manifest signed
  by the computer's key. Browsers still fetch `sw.js` themselves, so this
  raises the bar against a mis-issued certificate but does not stop it.

## Sheprd and the command line

- **Settings > Remote control > Web access through relay**: off by default.
  Turning it on opens a confirmation that says the computer's sign-in page
  will be reachable from the internet. Once on, the section shows the URL,
  a QR, the certificate's state and expiry, the CT watch's last check, and
  any alert.
- **CLI** (`sheprd-control-protocol`, `control/server`, `control/client`):
  - `sheprd remote web status`: per relay, the URL, the certificate's
    expiry, and the CT state.
  - `sheprd remote web enable [--relay NAME]` and `disable`: `enable` opens
    the confirmation dialog, `disable` acts at once.

  All with `--json` and `--window`, `--help`, tests, and `docs/cli.md`.
- `sheprd remote status --json` reports web access per relay.

## Threat model

| Who | Can | Cannot |
| --- | --- | --- |
| Network attacker, Wi-Fi, ISP | See addresses, timing, sizes, and the hostname (SNI) | Read or change the app or data |
| The relay, or anyone who takes it over | Drop or delay sessions; see which hostname gets traffic; refuse to issue | Read or change traffic: it never has the key. Use another relay's names: CAA |
| The relay's operator, mis-issuing for a name in its own zone | Get a certificate for its own key and serve altered JavaScript to new visits | Do it unseen: the daemon's CT watch catches it and turns web access off. Until then, altered JavaScript could use a passkey while the user is on the page, so the defense here is detection, not prevention |
| A stranger on a public relay | Serve their own page under the relay domain | Touch other computers' origins, storage, or passkeys; the website on `sheprd.org` |
| Whoever controls the DNS zone | Issue for any name in it | Hide it from CT |
| A stolen phone | What the device could do until revoked | Refresh without the passkey's user verification |

For a private relay you run yourself, the operator is you; what remains is a
compromised droplet or DNS account, which the CT watch reports.

## Testing

- **Relay:** the ClientHello parser against malformed, fragmented, slow, and
  oversized input, and a fuzz target; SNI routing over real sockets (own
  name to a stub Caddy with PROXY v2, a web label to a listener, unknown or
  off labels closed identically); web session limits and the deny list; CSR
  checks (wrong name, extra names, bad signature, wrong key type, over the
  allowance).
- **ACME:** opt-in tests against Pebble, Let's Encrypt's test server, through
  the relay front with TLS-ALPN-01, including renewal and a restarted relay.
- **Daemon:** chain checks (wrong key, wrong name, untrusted root, expired);
  the resolver choosing by SNI; the passkey flow with a software
  authenticator; the CT watch against a stub with a foreign key, an empty
  answer, and a failed query.
- **End to end (opt-in):** a browser through Playwright against a local
  relay and Pebble, from pairing to a chat and a push subscription.
- **Native QA:** iPhone Safari and Android Chrome, installed to the home
  screen, on cellular with Wi-Fi off: pairing, passkey, push, reconnecting
  after the phone sleeps.

## Phases

1. **Relay:** the TCP front, web sessions, CSR issuance with TLS-ALPN-01,
   rationing, limits, the deny list, `/v1/info` `web`, and the deployment
   changes (wildcard record, CAA output, Caddy behind PROXY v2). Register
   `sheprd-relay.dev` and request the rate limit increase now, since it
   takes weeks.
2. **Daemon:** key and CSR, chain checks, renewal, serving web sessions,
   origins, and pairing on the web origin.
3. **Shipped together:** required passkeys, the CT watch, the Sheprd
   setting, the CLI, and docs (`docs/cli.md`, the skill's command table,
   `docs/remote-control-plan.md`, `docs/relay-plan.md`). Web access is not
   offered to users before this phase is complete.
4. **After:** the signed service worker shell, web access on Sheprd's public
   relays (Still open, 1), and Public Suffix List entries.
