import SheprdRemoteCore
import Testing

@Suite struct CollapsedComputersTests {
    @Test func aComputerIsOpenUntilCollapsed() {
        var collapsed = CollapsedComputers()
        #expect(!collapsed.contains("a"))
        collapsed.toggle("a")
        #expect(collapsed.contains("a"))
        collapsed.toggle("a")
        #expect(!collapsed.contains("a"))
    }

    @Test func oneStillOpenMakesToggleAllCollapseThemAll() {
        var collapsed = CollapsedComputers(["a"])
        collapsed.toggleAll(["a", "b"])
        #expect(collapsed.allCollapsed(["a", "b"]))
    }

    @Test func allCollapsedMakesToggleAllOpenThem() {
        var collapsed = CollapsedComputers(["a", "b"])
        collapsed.toggleAll(["a", "b"])
        #expect(collapsed.devices.isEmpty)
    }

    @Test func toggleAllLeavesComputersAFilterHides() {
        var collapsed = CollapsedComputers(["hidden"])
        collapsed.toggleAll(["a", "b"])
        #expect(collapsed.devices == ["a", "b", "hidden"])
        collapsed.toggleAll(["a", "b"])
        #expect(collapsed.devices == ["hidden"])
    }

    @Test func noComputersAreNeverAllCollapsed() {
        #expect(!CollapsedComputers(["a"]).allCollapsed([String]()))
    }

    @Test func aComputerPairedLaterKeepsAllCollapsedFalse() {
        #expect(!CollapsedComputers(["a"]).allCollapsed(["a", "new"]))
    }

    @Test func forgettingDropsTheComputer() {
        var collapsed = CollapsedComputers(["a", "b"])
        collapsed.forget("a")
        #expect(collapsed.devices == ["b"])
    }

    @Test func pruningDropsOnlyMissingKeysUnderThePrefix() {
        var collapsed = CollapsedComputers(["workspace/a/1", "workspace/a/2", "workspace/b/1", "priority/blocked"])
        collapsed.prune(prefix: "workspace/a/", keeping: ["2"])
        #expect(collapsed.devices == ["workspace/a/2", "workspace/b/1", "priority/blocked"])
        collapsed.prune(prefix: "workspace/a/", keeping: [])
        #expect(collapsed.devices == ["workspace/b/1", "priority/blocked"])
    }
}
