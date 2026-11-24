-- A passive frame around the workspace, not an overlay dialog. The four narrow
-- images play in GPUI; animation never publishes a provider tick or rebuilds Luau.
local FILL = { left = 0, top = 0, right = 0, bottom = 0 }
local SIDES = {
	{ name = "top", horizontal = true, pins = { top = 0, left = 0, right = 0 }, measure = "width" },
	{ name = "right", pins = { top = 0, right = 0, bottom = 0 } },
	{ name = "bottom", horizontal = true, pins = { left = 0, right = 0, bottom = 0 } },
	{ name = "left", pins = { left = 0, top = 0, bottom = 0 }, measure = "height" },
}
local LIGHTS = {
	working = { color = "#63baff", depth = 5, base = 0.20 },
	blocked = { color = "#ffad53", depth = 4, base = 0.28 },
	done = { color = "#67d9a6", depth = 4, base = 0.22 },
	idle = { color = "#90a5bb", depth = 2, base = 0.12 },
}

local function strip(side, state, status, light, tint)
	local length = if side.horizontal then state.width else state.height
	local width = if side.horizontal then length else light.depth
	local height = if side.horizontal then light.depth else length
	local sprite = if status == "working" then `working-{side.name}`
		elseif status == "done" then (if side.horizontal then "done-horizontal" else "done-vertical")
		else status
	return ui.column {
		float = side.pins,
		width = if side.horizontal then nil else light.depth,
		height = if side.horizontal then light.depth else nil,
		gap = "none",
		ui.row { float = FILL, style = { background = tint, opacity = light.base } },
		-- Only these two strips measure. The viewer reports on resize, in layout
		-- pixels; plain images need both dimensions to stretch rather than letterbox.
		if side.measure then ui.image { src = "clear.png", fit = "fill", measure = { edge = side.measure } } else false,
		if length and length > 0 then ui.row {
			key = status, float = FILL, gap = "none",
			ui.image { src = sprite .. ".webp", fit = "fill", width = width, height = height },
		} else false,
	}
end

return mod {
	-- Geometry belongs to this window, not to the previous persisted session.
	init = function() return {} end,
	update = function(state, msg)
		if type(msg) ~= "table" then return end
		if msg.edge == "width" then state.width = msg.box_width end
		if msg.edge == "height" then state.height = msg.box_height end
	end,
	view = function(state, app)
		-- The same aggregate as the worktree's sidebar dot. `workspaces` is
		-- already scoped to the selected device and includes folded worktrees.
		local sheprd = app.sheprd
		for _, workspace in (sheprd and sheprd.workspaces) or {} do
			if not workspace.focused then continue end
			local status = workspace.agent_status
			local light = LIGHTS[status]
			if not light then break end
			local theme = app.sheprd_theme or {}
			local tint = (theme.status and theme.status[status]) or light.color
			local frame = { float = FILL, gap = "none" }
			for _, side in SIDES do table.insert(frame, strip(side, state, status, light, tint)) end
			return ui.column(frame)
		end
		return ui.row { float = "top-left", width = 0, height = 0 }
	end,
}
