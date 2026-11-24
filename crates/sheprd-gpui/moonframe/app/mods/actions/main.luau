-- The older native host accepts these commands but has no sidebar_actions view.
local function link(label, command, theme)
	return ui.row {
		class = "sheprd-link",
		msg = { command = command },
		to = "provider:sheprd",
		ui.text { label, style = { color = theme.muted } },
	}
end

return mod {
	init = function(previous) return previous or {} end,
	update = function() end,
	view = function(_, app)
		local sheprd = app.sheprd or {}
		local theme = app.sheprd_theme or {}
		local layout = sheprd.layout or {}
		return ui.row {
			class = "sheprd-left-actions",
			align = "center",
			shrink = false,
			style = {
				background = theme.sidebar,
				font = "mono",
				size = theme.sidebar_size,
				padding = { x = (layout.padding or 8) + (layout.inset or 0), y = 4 },
			},
			link("new", "new_workspace", theme),
			ui.spacer(),
			link("menu", "command_palette", theme),
		}
	end,
}
