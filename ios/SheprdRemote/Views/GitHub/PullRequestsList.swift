import SheprdGitHub
import SheprdRemoteCore
import SwiftUI

/// What the Workspaces tab lists: the computers' workspaces, or the
/// account's pull requests across GitHub.
enum WorkspacesShows: String {
    case workspaces, pullRequests

    static let key = "workspacesShows"
}

/// The switch on top of the Workspaces tab.
struct WorkspacesShowsPicker: View {
    @Binding var shows: WorkspacesShows

    var body: some View {
        Section {
            Picker("Show", selection: $shows.animation(.snappy)) {
                Text("Workspaces").tag(WorkspacesShows.workspaces)
                Text("Pull Requests").tag(WorkspacesShows.pullRequests)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets())
        }
    }
}

/// The pull requests the github.com account wrote, and those asking for its
/// review, on any repository, including ones no computer has a workspace
/// for. Each says where its branch is checked out. Review requests come
/// first while there are any: they wait on the person.
struct PullRequestsList: View {
    @Environment(AppModel.self) private var model
    @Binding var shows: WorkspacesShows
    /// A workspace opened from a row's menu.
    @State private var opened: WorkspaceKey?

    private var direct: GitHubDirect { GitHubDirect.shared }

    var body: some View {
        List {
            WorkspacesShowsPicker(shows: $shows)
            if let lists = direct.lists.lists {
                if case .failed = direct.lists {
                    Section {
                        Label("GitHub could not be read. These may be out of date.", systemImage: "exclamationmark.triangle")
                            .font(.footnote).foregroundStyle(.secondary)
                    }
                }
                if lists.reviewRequested.isEmpty {
                    mine(lists)
                } else {
                    requested(lists)
                    mine(lists)
                }
            }
        }
        .listStyle(.insetGrouped)
        .listSectionSpacing(14)
        .refreshable { await direct.refreshLists(source: model.fleet.gitHubLenders) }
        .overlay { empty }
        .navigationDestination(for: PullRequestRoute.self) { route in
            PullRequestDetailScreen(route: route, title: direct.title(for: route))
        }
        .navigationDestination(item: $opened) { WorkspaceScreen(key: $0) }
    }

    @ViewBuilder private func mine(_ lists: PullRequestLists) -> some View {
        if !lists.mine.isEmpty {
            Section {
                ForEach(lists.mine) { row($0, showsAuthor: false) }
            } header: {
                header("Mine", shown: lists.mine.count, total: lists.mineTotal)
            }
        }
    }

    @ViewBuilder private func requested(_ lists: PullRequestLists) -> some View {
        Section {
            ForEach(lists.reviewRequested) { row($0, showsAuthor: true) }
        } header: {
            header("Review requested", shown: lists.reviewRequested.count, total: lists.reviewRequestedTotal)
        }
    }

    private func header(_ title: String, shown: Int, total: Int) -> some View {
        HStack {
            Text(title)
            Spacer()
            if total > shown { Text("\(shown) of \(total)").textCase(nil) }
        }
    }

    private func row(_ pull: GitHubSearchPull, showsAuthor: Bool) -> some View {
        let places = ComputerWorkspaces.places(of: pull, in: computers)
        return NavigationLink(value: PullRequestRoute(pull)) {
            PullRequestListRow(pull: pull, showsAuthor: showsAuthor, places: places)
        }
        .contextMenu {
            ForEach(places, id: \.self) { place in
                Button("Open Workspace on \(place.computer)", systemImage: "laptopcomputer") {
                    opened = WorkspaceKey(device: place.device, workspace: place.workspace)
                }
            }
            if let url = pull.url {
                Link(destination: url) { Label("Open on GitHub", systemImage: "safari") }
                ShareLink(item: url)
            }
        }
    }

    private var computers: [ComputerWorkspaces] {
        #if DEBUG
        if GitHubSample.enabled { return GitHubSample.computers }
        #endif
        return model.fleet.computerWorkspaces
    }

    @ViewBuilder private var empty: some View {
        switch direct.lists {
        case .noAccount:
            ContentUnavailableView {
                Label("No GitHub account", systemImage: "arrow.triangle.pull")
            } description: {
                Text("Sign in to GitHub on this \(UIDevice.current.model), or let a paired computer lend its sign-in.")
            } actions: {
                NavigationLink("Open Settings › GitHub") { GitHubSettingsView() }
                    .buttonStyle(.borderedProminent)
            }
        case .unread:
            ProgressView()
        case let .read(lists) where lists.isEmpty:
            ContentUnavailableView("No open pull requests", systemImage: "arrow.triangle.pull",
                                   description: Text("Pull requests you opened, and those asking for your review, appear here."))
        case .failed(nil):
            ContentUnavailableView("GitHub could not be read", systemImage: "exclamationmark.triangle",
                                   description: Text("Pull down to try again."))
        case .read, .failed:
            EmptyView()
        }
    }
}

/// A pull request's row: repository and number, title, review or author,
/// checks, and where its branch is checked out.
private struct PullRequestListRow: View {
    var pull: GitHubSearchPull
    var showsAuthor: Bool
    var places: [WorkspacePlace]

    var body: some View {
        let look = PullRequestLook(pull.state, draft: pull.isDraft)
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 6) {
                Image(systemName: look.symbol).font(.caption.weight(.semibold)).foregroundStyle(look.color)
                Text(verbatim: "\(pull.repository) #\(pull.number)").font(.caption.monospaced())
                    .foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                Spacer(minLength: 4)
                Text(verbatim: PullRequestLink.age(pull.updatedAt)).font(.caption).foregroundStyle(.secondary)
            }
            Text(verbatim: pull.title).lineLimit(2)
            HStack(spacing: 8) {
                if showsAuthor, let author = pull.author {
                    PullRequestAvatar(login: author, size: 16)
                    Text(verbatim: author).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                } else if let review = pull.reviewDecision.look {
                    IconText(review.label, systemImage: review.symbol)
                        .font(.caption.weight(.medium)).foregroundStyle(review.color).lineLimit(1)
                } else {
                    Text(look.label).font(.caption).foregroundStyle(.secondary)
                }
                Spacer(minLength: 4)
                CheckCountsView(counts: pull.checks)
            }
            whereLine
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }

    private var whereLine: some View {
        Group {
            if let first = places.first {
                let others = Set(places.map(\.device)).subtracting([first.device]).count
                IconText(others == 0 ? "Workspace on \(first.computer)" : "Workspace on \(first.computer) and \(others) more",
                         systemImage: "laptopcomputer")
            } else {
                IconText("No workspace on any computer", systemImage: "icloud")
            }
        }
        .font(.caption2)
        .foregroundStyle(places.isEmpty ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.secondary))
        .lineLimit(1)
    }
}

/// Failing, pending, and passed checks as small colored numbers.
private struct CheckCountsView: View {
    var counts: GitHubSearchPull.CheckCounts

    var body: some View {
        HStack(spacing: 8) {
            if counts.failing > 0 { count(counts.failing, "xmark.circle.fill", .red) }
            if counts.pending > 0 { count(counts.pending, "clock.fill", .yellow) }
            if counts.passed > 0 { count(counts.passed, "checkmark.circle.fill", .green) }
        }
        .font(.caption.weight(.semibold).monospacedDigit())
        .fixedSize()
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(counts.failing) failing, \(counts.pending) pending, \(counts.passed) passed checks")
    }

    private func count(_ value: Int, _ symbol: String, _ color: Color) -> some View {
        HStack(spacing: 3) {
            Image(systemName: symbol).foregroundStyle(color)
            Text(verbatim: "\(value)").foregroundStyle(.secondary)
        }
    }
}

/// An icon and its text side by side. A `Label` in a list row takes the
/// row's icon column, which spreads these small captions apart.
struct IconText: View {
    var title: String
    var systemImage: String

    init(_ title: String, systemImage: String) {
        self.title = title
        self.systemImage = systemImage
    }

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: systemImage)
            Text(verbatim: title)
        }
    }
}
