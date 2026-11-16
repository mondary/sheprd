// swift-tools-version: 6.0
// The app's logic that needs no UIKit, ActivityKit, or Keychain: pairing
// links, fingerprints, base64url, session signatures, the Live Activity
// content, sealed notifications, the API's JSON, and the transcript
// reducer. It builds for macOS too, so `swift test` runs it without a
// simulator.
//
// `SheprdGitHub` is the phone's own GitHub client on top of it: hosts,
// leased and signed-in tokens, the REST and GraphQL reads, pull requests,
// checks, and their logs. It depends on `SheprdRemoteCore`, never the other
// way, and only the app links it, not the widgets, watch, or extensions.

import PackageDescription

let package = Package(
    name: "SheprdRemoteCore",
    platforms: [.iOS("26.0"), .macOS("26.0"), .watchOS("26.0")],
    products: [
        .library(name: "SheprdGitHub", targets: ["SheprdGitHub"]),
        .library(name: "SheprdRemoteCore", targets: ["SheprdRemoteCore"]),
    ],
    targets: [
        .target(name: "SheprdRemoteCore"),
        .target(name: "SheprdGitHub", dependencies: ["SheprdRemoteCore"]),
        .testTarget(
            name: "SheprdRemoteCoreTests",
            dependencies: ["SheprdRemoteCore"],
            resources: [.copy("Fixtures")]
        ),
        .testTarget(
            name: "SheprdGitHubTests",
            dependencies: ["SheprdGitHub", "SheprdRemoteCore"],
            resources: [.copy("Fixtures")]
        ),
    ]
)
