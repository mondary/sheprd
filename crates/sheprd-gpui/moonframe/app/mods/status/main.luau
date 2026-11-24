local menu = use "menu"
local OPTIONS = {
	{ id = "about", label = "About SheprdPK", icon = "info" },
	{ id = "update", label = "Check for Updates…", icon = "refresh-cw" },
}

local function item(icon: string, text: string, color: string, msg: any, to: string, hint: string?)
	return ui.row {
		class = "status-item", align = "center", shrink = false,
		msg = msg, to = to, tooltip = hint,
		style = { gap = 5 },
		ui.icon { name = icon, size = "large", style = { color = color } },
		ui.text { text, style = { color = color } },
	}
end

local function details_item(state, theme)
	local place = state.details_place or "right"
	local icon, hint
	if state.details_shown == false then
		icon = if place == "left" then "panel-left-open" else "panel-right-open"
		hint = `Details are hidden: click to show them on the {place}`
	elseif place == "left" then
		icon, hint = "panel-left", "Details on the left: click to move them to the right"
	else
		icon, hint = "panel-right", "Details on the right: click to hide them"
	end
	return item(icon, "Details", theme.subtext, "toggle-details", "shell", hint)
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
	init = function()
		return {}, { { to = "shell", msg = "details-state" } }
	end,
	update = function(state, msg)
		if msg == "toggle-version" then
			state.choice = if state.choice then nil else 1
		elseif msg == "close-version" then
			state.choice = nil
		elseif type(msg) == "table" and msg.details_place ~= nil then
			state.details_place = msg.details_place
			state.details_shown = msg.details_shown == true
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
			class = "sheprd-status", grow = true, align = "stretch",
			style = { background = theme.surface, gap = 14, padding = { left = 12, right = 12 } },
			if sheprd.status_version then ui.column {
				grow = true, gap = "none",
				ui.view { name = "sheprd", id = "status-metrics", props = { region = "status-metrics" } },
			} else ui.spacer(),
			ui.row {
				align = "center", shrink = false, style = { gap = 14 },
				if sheprd.details_visible == nil then details_item(state, theme) else false,
				item("palette", "Theme", theme.subtext, { command = "themes" }, "provider:sheprd"),
				item("keyboard", "Shortcuts", theme.subtext, { command = "keybindings" }, "provider:sheprd"),
				item("command", "Commands", theme.subtext, { command = "command_palette" }, "provider:sheprd"),
				version_chip(state, sheprd.status_version or "SheprdPK", theme),
			},
		}
	end,
}
