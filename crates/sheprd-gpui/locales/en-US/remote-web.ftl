# Turning on web access through a private relay: the question, asked in the
# window for `sheprd remote web enable` and in Settings > Remote control >
# Relays alike, and the status line's error when it fails. $relay is the
# relay's name or address.

remote-web-title = Turn on web access through { $relay }?
remote-web-detail = This computer's sign-in page becomes reachable from the internet at its web address through { $relay }. Devices pair there with a passkey.
remote-web-alert = A certificate for this web address with another key was seen in a Certificate Transparency log, so web access was turned off. Turning it on again trusts the relay's operator.
remote-web-cancel = Cancel
remote-web-submit = Turn On
# $error says why.
remote-web-failed = Could not turn web access on again through { $relay }: { $error }
