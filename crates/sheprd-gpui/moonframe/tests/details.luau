-- check-moonframe-details.sh supplies the production shell, status and details modules.
local icon = "~/.cache/sheprd/images/fixture.png"
local app = {
	sheprd = {
		workspaces = { { focused = true, id = "w", label = "repo", repo_key = "/repo/.git" } },
		agents = { { workspace = "w", name = "agent in repo", pane = "p", status = "working" } },
	},
	sheprd_theme = { sidebar = "#345678" },
	files = { available = true, path = "/repo", root = "repo", dirs = { [""] = { { name = "entry.txt" } } } },
	gitcounts = { counts = { ["/repo"] = { icon = icon, changes = { { path = "changed.txt", status = "M" } } } } },
	gitgraph = { commits = { { sha = "abc", parents = {}, subject = "first commit" } } },
}

local function find(node, predicate)
	if type(node) ~= "table" or node.visible == false then return nil end
	if predicate(node) then return node end
	for _, child in ipairs(node) do
		local hit = find(child, predicate)
		if hit then return hit end
	end
	if node.type == "list" then
		for i = 1, node.count do
			local hit = find(node.row(i), predicate)
			if hit then return hit end
		end
	end
	return nil
end

local function text(node, label)
	return find(node, function(n) return n.text == label or n.label == label end)
end

local function control(node, label)
	return find(node, function(n) return n.msg ~= nil and text(n, label) end)
end

return mod {
	styles = details.styles,
	view = function()
		local layout = shell.init(nil)
		local function opened()
			return find(shell.view(layout, app), function(n) return n.type == "mount" and n.mod == "details" end) ~= nil
		end
		local function status_toggle()
			return control(status.view({}, app), "Details")
		end
		local toggle = assert(status_toggle(), "host without a titlebar toggle needs a status button")
		assert(toggle.to == "shell" and not opened())
		shell.update(layout, toggle.msg)
		assert(opened(), "status button did not open Details")
		shell.update(layout, toggle.msg)
		assert(not opened(), "status button did not close Details")
		shell.update(layout, toggle.msg) -- Local open must not override a native close.
		for _, visible in { false, true, false } do
			app.sheprd.details_visible = visible
			assert(opened() == visible and not status_toggle(), "native titlebar must own visibility")
		end
		app.sheprd.details_visible = nil
		local bar = status.init(nil)
		status.update(bar, "toggle-version")
		local _, about = status.update(bar, "choose-version")
		assert(about[1].msg.command == "about" and bar.choice == nil)
		status.update(bar, "toggle-version")
		status.update(bar, "move-version")
		local _, check = status.update(bar, "choose-version")
		assert(check[1].msg.check_for_updates == true and bar.choice == nil)
		-- Exercise the real startup path after the native provider publishes.
		app.sheprd.status_version = "2026.11.04"
		local status_closed = status.view(bar, app)
		status.update(bar, "toggle-version")
		local status_menu = status.view(bar, app)
		local pages = { status_closed, status_menu }

		local main = function(n)
			return n.type == "view" and n.name == "sheprd" and type(n.props) == "table" and n.props.region == "main"
		end
		local function details_side(tree)
			local split = assert(find(tree, function(n)
				return n.type == "hsplit" and (n.id == "body-left" or n.id == "body-right")
			end), "missing the body split")
			local column, tabs
			for index, pane in ipairs(split) do
				if find(pane, function(n) return n.type == "mount" and n.mod == "details" end) then column = index end
				if find(pane, main) then tabs = index end
			end
			assert(tabs, "the terminals are missing")
			if not column then return "hidden" end
			return if column < tabs then "left" else "right"
		end
		local function details_icon()
			local row = assert(control(status.view(bar, app), "Details"), "missing the Details button")
			return assert(find(row, function(n) return n.type == "icon" end)).name
		end
		layout.details, layout.details_side = true, "right"
		assert(details_side(shell.view(layout, app)) == "right")
		local news = select(2, shell.update(layout, "toggle-details"))
		assert(not layout.details and layout.details_side == "left", "a shown column hides first")
		assert(news[1].to == "status" and news[1].msg.details_place == "left" and news[1].msg.details_shown == false)
		assert(details_side(shell.view(layout, app)) == "hidden")
		status.update(bar, news[1].msg)
		assert(details_icon() == "panel-left-open", "a hidden column shows where a click opens it")
		news = select(2, shell.update(layout, "toggle-details"))
		assert(layout.details and news[1].msg.details_place == "left" and news[1].msg.details_shown == true)
		table.insert(pages, shell.view(layout, app))
		assert(details_side(pages[#pages]) == "left", "the column sits between the workspaces and the tabs")
		status.update(bar, news[1].msg)
		assert(details_icon() == "panel-left")
		news = select(2, shell.update(layout, "toggle-details"))
		assert(layout.details and news[1].msg.details_place == "right")
		table.insert(pages, shell.view(layout, app))
		assert(details_side(pages[#pages]) == "right")
		status.update(bar, news[1].msg)
		assert(details_icon() == "panel-right")
		news = select(2, shell.update(layout, "details-state"))
		assert(news[1].to == "status" and news[1].msg.details_place == "right")
		layout.details = false
		table.insert(pages, shell.view(layout, app))
		assert(details_side(pages[#pages]) == "hidden" and details_icon() == "panel-right")

		local state = details.init(nil)
		local page = details.view(state, app)
		for _, pair in { { "Files", "entry.txt" }, { "Agents", "agent in repo" }, { "Git", "changed.txt" } } do
			local tab = assert(control(page, pair[1]), "missing tab: " .. pair[1])
			state = details.update(state, tab.msg) or state
			page = details.view(state, app)
			assert(text(page, pair[2]), "missing content on " .. pair[1])
			if pair[1] == "Agents" then
				local function spinning()
					local row = assert(control(details.view(state, app), "agent in repo"))
					return find(row, function(n) return n.type == "icon" and n.spin end) ~= nil
				end
				assert(spinning(), "a working agent has no moving indicator")
				for _, status in { "blocked", "done", "idle", "unknown" } do
					app.sheprd.agents[1].status = status
					assert(not spinning(), status .. " agent still has a moving indicator")
				end
			end
			assert(find(page, function(n) return n.type == "image" and n.src == icon end), "missing project icon")
			table.insert(pages, page)
		end
		assert(text(page, "No branch") and text(page, "History") and text(page, "first commit"), "incomplete Git page")
		local workspace = app.sheprd.workspaces[1]
		workspace.repo_key = nil
		workspace.local_cwd = "/suite"
		state = details.update(state, { topic = "provider/sheprd", value = app.sheprd })
		local chrome = { path = "/suite/chrome", name = "chrome", branch = "main", dirty = false, count = 0 }
		local mac = { path = "/suite/mac", name = "mac", branch = "dev", dirty = true, count = 1 }
		local linux = { path = "/suite/linux", name = "linux", branch = "trunk", dirty = false, count = 0 }
		app.repositories = { scope = "/suite", repos = { chrome, mac, linux }, selected = {
			path = chrome.path, branch = chrome.branch, changes = {}, commits = {},
		} }
		page = details.view(state, app)
		assert(text(page, "No uncommitted changes") and control(page, "chrome") and control(page, "linux"))
		local next_state, sends = details.update(state, assert(control(page, "mac")).msg)
		state = next_state
		assert(sends[1].msg.repository == mac.path)
		assert(text(details.view(state, app), "Reading repositories…"))
		app.repositories.selected = {
			path = mac.path, branch = mac.branch, dirty = true,
			changes = { { path = "mac.txt", status = "M" } },
			commits = { { sha = "def", parents = {}, subject = "mac commit" } },
		}
		page = details.view(state, app)
		assert(text(page, "mac.txt") and text(page, "mac commit") and not text(page, "first commit"))
		table.insert(pages, page)
		workspace.local_cwd = "/different"
		state = details.update(state, { topic = "provider/sheprd", value = app.sheprd })
		assert(not text(details.view(state, app), "mac.txt"))
		-- Hand all three views to Moonframe: invalid child properties are not errors until it reads the tree.
		return ui.column(pages)
	end,
}
