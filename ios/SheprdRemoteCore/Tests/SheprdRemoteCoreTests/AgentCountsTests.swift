import Testing
@testable import SheprdRemoteCore

/// Home's counts above its list: one per status, in the priority order.
struct AgentCountsTests {
    private static func counts(_ statuses: [Home.AgentStatus]) -> [String] {
        AgentSort.counts(of: statuses) { status in
            AgentSort.Facts(status: status, date: nil, title: "", group: "g", groupTitle: "g")
        }
        .map { "\($0.status.rawValue) \($0.count)" }
    }

    @Test func countsFollowThePriorityOrder() {
        #expect(Self.counts([.idle, .done, .working, .done, .blocked]) == ["blocked 1", "done 2", "working 1", "idle 1"])
    }

    @Test func anUnknownStatusCountsAsIdle() {
        #expect(Self.counts([.unknown, .idle]) == ["idle 2"])
    }

    @Test func aStatusNoneHasIsLeftOut() {
        #expect(Self.counts([.done]) == ["done 1"])
        #expect(Self.counts([]).isEmpty)
    }
}
