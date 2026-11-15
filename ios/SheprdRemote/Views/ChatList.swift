import SheprdRemoteCore
import SwiftUI

/// Every chat across computers in the order the person chose, a question
/// answered in place. Agents run in terminal tabs join them, so Home lists
/// every agent the computers run. Home shows it.
///
/// Counts by status sit above the sections, and each section folds to its
/// header as a computer does in the Workspaces tab.
struct ChatList: View {
    @Environment(AppModel.self) private var model
    @AppStorage(HomeRow.sortKey) private var sort = AgentSort.priority
    var sections: [AgentSort.Section<HomeRow>]
    /// The chat a long press asked to close, waiting for confirmation.
    @State private var closing: ChatItem?
    @State private var problem: String?

    /// What `CollapsedStore.home` keeps for `section`: a workspace and a
    /// status may share an id, so the order names it too.
    static func key(_ section: AgentSort.Section<HomeRow>, sort: AgentSort) -> String {
        "\(sort.rawValue)/\(section.id)"
    }

    var body: some View {
        let counts = HomeRow.counts(chats: model.fleet.chatItems, agents: model.fleet.agentTerminals)
        ScrollViewReader { proxy in
            List {
                UnpairedCards()
                if !counts.isEmpty {
                    CountChips(counts: counts) { status in open(status, proxy: proxy) }
                        .listRowInsets(EdgeInsets())
                        .listRowBackground(Color.clear)
                        .listRowSeparator(.hidden)
                }
                ForEach(sections) { section in
                    let key = Self.key(section, sort: sort)
                    let folded = model.collapsed.home.contains(key)
                    Section {
                        if !folded { ForEach(section.items) { row($0) } }
                    } header: {
                        SectionHeader(title: section.title, count: section.items.count, folded: folded) {
                            withAnimation(.snappy) { model.collapsed.home.toggle(key) }
                        }
                        .id(key)
                    }
                }
            }
        }
        .refreshable { model.fleet.networkChanged() }
        .confirmationDialog(
            "Close \(closing?.chat.title ?? "this chat")?",
            isPresented: Binding(get: { closing != nil }, set: { if !$0 { closing = nil } }),
            titleVisibility: .visible,
            presenting: closing
        ) { item in
            Button("Close Chat", role: .destructive) { close(item) }
        } message: { _ in
            Text("Its pane closes in Sheprd and the agent stops.")
        }
        .alert("The chat did not close", isPresented: Binding(get: { problem != nil }, set: { if !$0 { problem = nil } })) {
        } message: {
            Text(problem ?? "")
        }
    }

    @ViewBuilder
    private func row(_ row: HomeRow) -> some View {
        // A section names the workspace when they are grouped by it.
        let grouped = sort == .workspace
        switch row {
        case let .chat(item):
            Group {
                if let question = item.question {
                    // The card draws its own background and border.
                    AnswerCard(item: item, question: question)
                        .listRowInsets(EdgeInsets(top: 4, leading: 0, bottom: 4, trailing: 0))
                        .listRowBackground(Color.clear)
                        .listRowSeparator(.hidden)
                } else {
                    NavigationLink(value: Route.chat(item.key, title: item.chat.title)) {
                        ChatRowLabel(item: item, showsComputer: !grouped, showsWorkspace: !grouped)
                    }
                }
            }
            .contextMenu {
                Button("Close Chat", systemImage: "xmark.circle", role: .destructive) { closing = item }
            }
        case let .agent(item):
            if let route = item.route {
                NavigationLink(value: route) { AgentTerminalRowLabel(item: item, showsComputer: !grouped, showsWorkspace: !grouped) }
            }
        }
    }

    /// A count's tap shows its section, opening it if folded. Only the
    /// priority order has a section per status; the others keep their place.
    private func open(_ status: Home.AgentStatus, proxy: ScrollViewProxy) {
        guard sort == .priority, let section = sections.first(where: { $0.id == status.rawValue }) else { return }
        let key = Self.key(section, sort: sort)
        withAnimation(.snappy) {
            if model.collapsed.home.contains(key) { model.collapsed.home.toggle(key) }
            proxy.scrollTo(key, anchor: .top)
        }
    }

    private func close(_ item: ChatItem) {
        Task {
            do {
                try await model.fleet.closeChatPane(item.key)
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}

/// One chip per status with agents: its icon and how many. The section
/// headers below name them, so the chips stay short enough to fit a phone.
private struct CountChips: View {
    var counts: [(status: Home.AgentStatus, count: Int)]
    var tap: (Home.AgentStatus) -> Void

    var body: some View {
        HStack(spacing: 8) {
            ForEach(counts, id: \.status) { status, count in
                Button { tap(status) } label: {
                    HStack(spacing: 5) {
                        CountIcon(status: status)
                        Text(count, format: .number).fontWeight(.semibold).monospacedDigit()
                    }
                    .font(.subheadline)
                    .padding(.horizontal, 12).padding(.vertical, 7)
                    .background(.background, in: .capsule)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("\(count) \(status.label)")
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 4)
    }
}

/// A status as a filled round icon, idle included, which `StatusBadge`
/// leaves blank on rows.
private struct CountIcon: View {
    var status: Home.AgentStatus

    var body: some View {
        let symbol = switch status {
        case .blocked: "exclamationmark.circle.fill"
        case .working: "ellipsis.circle.fill"
        case .done: "checkmark.circle.fill"
        case .idle, .unknown: "moon.circle.fill"
        }
        Image(systemName: symbol).foregroundStyle(status.color)
    }
}

/// A section's title and count, folding it on a tap.
private struct SectionHeader: View {
    var title: String
    var count: Int
    var folded: Bool
    var toggle: () -> Void

    var body: some View {
        Button(action: toggle) {
            HStack(spacing: 6) {
                Image(systemName: "chevron.right").font(.caption.bold())
                    .rotationEffect(.degrees(folded ? 0 : 90))
                Text(title).lineLimit(1)
                Text(count, format: .number).foregroundStyle(.secondary).monospacedDigit()
                Spacer(minLength: 0)
            }
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(folded ? "Expand" : "Collapse") \(title), \(count)")
    }
}
