# The About box. "Herdr", "herdr.dev", and "GitHub" are names and stay as
# written, as do the author's name and the license.

# $version is the app's version, such as "0.42.0".
about-version = Version { $version }
# A build made from source has no release version; it names its commit instead.
# $commit is the short Git hash, such as "2f5a65a".
about-dev-commit = Dev build · { $commit }
about-dev-build = Dev build
# The read-only official release check a dev build shows.
about-official-latest = Latest official: { $version }
about-official-prerelease = Latest official: { $version } (pre-release)
about-official-checking = Checking the latest official release…
about-official-failed = Latest official release: check failed
about-official-none = Latest official: none published
about-official-check = Check again
about-summary = Native workspace for terminals and AI agents, powered by Herdr and herdr-core.
about-unaffiliated = An independent project, not affiliated with or endorsed by herdr.dev.
# The link to the app's own site.
about-website = Website
# A worktree build's pull request: $number is its number.
about-pull-request = PR #{ $number }
about-ok = OK
