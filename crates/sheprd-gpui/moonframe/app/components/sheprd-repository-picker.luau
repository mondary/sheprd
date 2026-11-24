return component {
	title = "Repository picker",
	props = { repos = "Checkout summaries", selected = "Selected checkout path", limited = "Scan limit reached", scope = "Project folder" },
	example = { repos = { { path = "/project/web", name = "web", branch = "main" } }, scope = "/project" },
	build = function(props)
		local rows = {}
		for _, repo in (props and props.repos) or {} do
			table.insert(rows, ui.row {
				class = if repo.path == props.selected then "details-tab details-tab-selected" else "details-tab",
				align = "center", gap = "small", shrink = false,
				msg = { select_repository = repo.path, git_scope = props.scope },
				tooltip = repo.path,
				ui.icon { name = "git-branch", size = "small" },
				ui.column {
					grow = true, gap = "none",
					ui.text { repo.name, truncate = true },
					ui.text { repo.branch, truncate = true, tone = "muted", size = "small" },
				},
				if repo.error then ui.badge { "Unavailable" }
					elseif repo.dirty then ui.badge { tostring(repo.count) }
					else ui.text { "Clean", tone = "muted", size = "small" },
			})
		end
		return ui.column {
			shrink = false, padding = "small", gap = "small",
			ui.text { "Repositories", weight = "semibold" },
			ui.column { height = math.min(#rows, 3) * 44, scroll = true, gap = "none", table.unpack(rows) },
			if props and props.limited then ui.text { "Repository scan limit reached", tone = "muted", size = "small" } else false,
		}
	end,
}
