-- MonoCode's shape, over Sheprd's MonoCode palette (themes/monocode).
-- Colors come from the theme; this file is the geometry and the few
-- interactive fills the toolkit would otherwise keep at Moonframe's defaults.
return style {
	-- MonoCode's controls: 6px, cards 10px. The theme keeps its own colors.
	radius = 6,

	rules = {
		-- Rows, tabs and toolbar buttons.
		item = { radius = 6 },
		["item:hover"] = { background = "#ffffff0d" },
		["item:selected"] = { background = "#ffffff1a" },
		button = { radius = 6 },
		badge = { radius = 999 },

		-- Fields: a 10% edge over a 3% wash, no heavy box.
		input = {
			background = "#ffffff08",
			border = true,
			border_color = "#ffffff1a",
			radius = 6,
			padding = { 5, 8 },
		},
		["input:hover"] = { border_color = "#ffffff26" },
		["input:focus"] = { border_color = "#ffffff33" },
		["input-placeholder"] = { color = "#ebebeb59" },
		["input-selection"] = { background = "#4da3f559" },
		["input-caret"] = { background = "#4da3f5" },

		-- Floating surfaces: MonoCode's #1c1c1c, 10px, one soft shadow.
		popover = {
			background = "#1c1c1c",
			radius = 10,
			border = true,
			border_color = "#ffffff1a",
			shadow = "0 16 48 -8 #00000099",
		},
		tooltip = {
			background = "#1c1c1c",
			radius = 6,
			border = true,
			border_color = "#ffffff1a",
			shadow = "0 8 24 -6 #00000080",
		},

		-- Panels as cards, and the hairline MonoCode draws between surfaces.
		panel = { radius = 10, border = true, border_color = "#ffffff0b" },
		["panel-header"] = { border = "none" },
		divider = { background = "#ffffff0b" },
		["overlay-backdrop"] = { background = "#000000", opacity = 0.35 },

		-- The rows Sheprd's own shell draws.
		[".worktree-tile"] = { radius = 6 },
		[".worktree-tile:selected"] = { background = "#ffffff1a" },
		[".worktree-row:hover"] = { background = "#ffffff0d" },
		[".agent-row:hover"] = { background = "#ffffff0d" },
		[".activity-tile:hover"] = { background = "#ffffff0d" },
		[".sheprd-head-tool"] = { radius = 6 },
		[".status-item:hover"] = { background = "#ffffff0d" },
		[".sheprd-link:hover"] = { opacity = 0.75 },
		[".usage-segment:hover"] = { background = "#ffffff0d" },
		[".usage-refresh:hover"] = { background = "#ffffff0d" },

		-- Edges: the hairline and its grip in MonoCode's accent.
		[".sheprd-left-edge-line"] = { background = "#ffffff0b" },
		[".sheprd-left-edge:hover"] = { background = "#4da3f559" },
		[".sheprd-grip"] = { background = "#4da3f5" },
	},
}
