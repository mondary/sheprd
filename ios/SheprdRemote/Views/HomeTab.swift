import SheprdRemoteCore
import SwiftUI

/// Home B: every agent in the order the person chose, a question answered
/// in place.
struct HomeTab: View {
    @Environment(AppModel.self) private var model
    @Environment(WindowState.self) private var window: WindowState?
    @AppStorage(HomeRow.sortKey) private var sort = AgentSort.priority
    @Binding var pairing: Bool

    var body: some View {
        let sections = HomeRow.sections(sort, chats: model.fleet.chatItems, agents: model.fleet.agentTerminals)
        ChatList(sections: sections)
            .nearbyBanner()
            .navigationBarTitleDisplayMode(.inline)
            .onAppear { window?.home = true }
            .onDisappear { window?.home = false }
            .overlay {
                if model.computers.isEmpty {
                    ContentUnavailableView {
                        Label("No computers yet", systemImage: "desktopcomputer")
                    } description: {
                        Text("In Sheprd, open Remote control > Pair device, then scan its code here.")
                    } actions: {
                        Button("Pair a computer") { pairing = true }.buttonStyle(.glassProminent)
                    }
                } else if model.fleet.chatItems.isEmpty, model.fleet.agentTerminals.isEmpty,
                          // Unpaired computers' cards fill Home instead.
                          !model.fleet.machines.contains(where: { $0.status == .revoked }) {
                    ContentUnavailableView("No chats", systemImage: "bubble.left.and.text.bubble.right",
                                           description: Text("Chats and agents running in Sheprd on your computers appear here."))
                }
            }
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { AgentSortMenu(sort: $sort) }
                if sections.count > 1 {
                    ToolbarItem(placement: .topBarTrailing) {
                        let shown = sections.map { ChatList.key($0, sort: sort) }
                        let folded = model.collapsed.home.allCollapsed(shown)
                        Button(
                            folded ? "Expand All" : "Collapse All",
                            systemImage: folded ? "rectangle.expand.vertical" : "rectangle.compress.vertical"
                        ) {
                            withAnimation(.snappy) { model.collapsed.home.toggleAll(shown) }
                        }
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Talk to Sheprd", systemImage: "waveform") { model.talk() }
                        .disabled(model.fleet.machines.isEmpty)
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("New Workspace", systemImage: "plus") { model.creating = .workspace(device: nil) }
                        .disabled(model.fleet.machines.isEmpty)
                }
            }
    }
}
