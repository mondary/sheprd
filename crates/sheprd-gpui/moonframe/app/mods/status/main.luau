-- Usage, resource meters and the cup are the same native controls as in
-- the classic window. The layout keeps its own shortcuts on the right.
local menu = use "menu"
local OPTIONS = {
	{ id = "about", label = "About SheprdPK", icon = "info" },
	{ id = "update", label = "Check for Updates…", icon = "refresh-cw" },
}

local function item(icon: string, text: string, color: string, msg: any, to: string)
	return ui.row {
		class = "status-item", align = "center", shrink = false,
		msg = msg, to = to, style = { gap = 5 },
		ui.icon { name = icon, size = "large", style = { color = color } },
		ui.text { text, style = { color = color } },
	}
end

local function version_chip(state, version, theme)
	return ui.popover {
		open = state.choice ~= nil, side = "top", align = "end", width = 218, gap = 4,
		dismiss = "close-version",
		keys = { up = "move-version", down = "move-version", enter = "choose-version" },
		content = menu { items = OPTIONS, highlighted = state.choice and OPTIONS[state.choice].id },
		ui.row {
			class = "status-item", align = "center", shrink = false,
			msg = "toggle-version", tooltip = "About and updates",
			style = { gap = 4, padding = { x = 6 }, radius = 4 },
			ui.text { version, style = { color = theme.muted } },
			ui.icon { name = "chevron-up", size = "small", style = { color = theme.muted } },
		},
	}
end

return mod {
	styles = { [".status-item:hover"] = { background = "secondary" } },
	init = function() return {} end,
	update = function(state, msg)
		if msg == "toggle-version" then
			state.choice = if state.choice then nil else 1
		elseif msg == "close-version" then
			state.choice = nil
		elseif msg == "move-version" and state.choice then
			state.choice = 3 - state.choice
		elseif state.choice then
			local pick = if msg == "choose-version" then OPTIONS[state.choice].id
				elseif type(msg) == "table" then msg.pick else nil
			if pick == "about" or pick == "update" then
				state.choice = nil
				return state, { { to = "provider:sheprd", msg = if pick == "about"
					then { command = "about" } else { check_for_updates = true } } }
			end
		end
		return state
	end,
	view = function(state, app)
		local sheprd = app.sheprd or {}
		local theme = app.sheprd_theme or {}
		return ui.row {
			class = "sheprd-status", grow = true, align = "center",
			style = { background = theme.surface, gap = 14, padding = { x = 12 } },
			if sheprd.status_version then ui.view {
				name = "sheprd", id = "status-metrics", grow = true, props = { region = "status-metrics" },
			} else ui.spacer(),
			if sheprd.details_visible == nil then item("panel-right", "Details", theme.subtext, "toggle-details", "shell") else false,
			item("palette", "Theme", theme.subtext, { command = "themes" }, "provider:sheprd"),
			item("keyboard", "Shortcuts", theme.subtext, { command = "keybindings" }, "provider:sheprd"),
			item("command", "Commands", theme.subtext, { command = "command_palette" }, "provider:sheprd"),
			version_chip(state, sheprd.status_version or "SheprdPK", theme),
		}
	end,
}
