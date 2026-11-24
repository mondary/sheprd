-- The right pane: the focused worktree, its uncommitted changes as a list of
-- files (each with its Material Icon Theme icon and its own ±), its agents,
-- and its commits as a graph. Each block sits in a pane whose divider drags,
-- and the project's icon closes the column.
local commit_graph = use "commit-graph"
local status_dot = use "sheprd-status-dot"

-- Each level of the Files tree steps in, and each row is this tall.
local INDENT = 12
local ROW_HEIGHT = 24

-- The folders this pane's Files tab has open, one path per line, so the
-- files provider lists them.
local function files_open_message(state: any)
	local paths = {}
	for path in state.files_open do
		table.insert(paths, path)
	end
	table.sort(paths)
	return { open = table.concat(paths, "\n"), owner = "details" }
end

-- One of the details column's tabs, without the built-in tab strip: the
-- column is narrow and the strip is only three fixed tabs.
local function tab_button(state: any, id: string, label: string, icon: string)
	local classes = { "details-tab" }
	if state.tab == id then
		table.insert(classes, "details-tab-selected")
	end
	return ui.row {
		class = classes,
		align = "center",
		gap = "small",
		msg = { select = id },
		ui.icon { name = icon, size = "small" },
		ui.text { label },
	}
end

-- Material Icon Theme's file icons, as the PNGs the material provider
-- keeps in the image cache. `material_icon` names one for a path.
local MATERIAL_EXT = {
	["adoc"] = "asciidoc.png",
	["asm"] = "assembly.png",
	["astro"] = "astro.png",
	["bak"] = "database.png",
	["bash"] = "console.png",
	["bib"] = "bibliography.png",
	["c"] = "c.png",
	["cc"] = "cpp.png",
	["cfg"] = "settings.png",
	["cjs"] = "javascript.png",
	["clj"] = "clojure.png",
	["cljs"] = "clojure.png",
	["conf"] = "settings.png",
	["cpp"] = "cpp.png",
	["cr"] = "crystal.png",
	["cs"] = "csharp.png",
	["css"] = "css.png",
	["csv"] = "table.png",
	["cxx"] = "cpp.png",
	["dart"] = "dart.png",
	["diff"] = "diff.png",
	["dmg"] = "disc.png",
	["env"] = "tune.png",
	["eot"] = "font.png",
	["erl"] = "erlang.png",
	["ex"] = "elixir.png",
	["exs"] = "elixir.png",
	["fish"] = "console.png",
	["fs"] = "fsharp.png",
	["fsx"] = "fsharp.png",
	["gif"] = "image.png",
	["go"] = "go.png",
	["gql"] = "graphql.png",
	["gradle"] = "gradle.png",
	["graphql"] = "graphql.png",
	["gz"] = "zip.png",
	["h"] = "h.png",
	["hpp"] = "hpp.png",
	["hs"] = "haskell.png",
	["htm"] = "html.png",
	["html"] = "html.png",
	["icns"] = "image.png",
	["ico"] = "image.png",
	["ini"] = "settings.png",
	["ipynb"] = "jupyter.png",
	["java"] = "java.png",
	["jl"] = "julia.png",
	["jpeg"] = "image.png",
	["jpg"] = "image.png",
	["js"] = "javascript.png",
	["json"] = "json.png",
	["jsonc"] = "json.png",
	["jsx"] = "react.png",
	["kt"] = "kotlin.png",
	["kts"] = "kotlin.png",
	["less"] = "less.png",
	["lock"] = "lock.png",
	["log"] = "log.png",
	["lua"] = "lua.png",
	["luau"] = "luau.png",
	["m"] = "objective-c.png",
	["md"] = "markdown.png",
	["mdx"] = "mdx.png",
	["mjs"] = "javascript.png",
	["mm"] = "objective-cpp.png",
	["mov"] = "video.png",
	["mp3"] = "audio.png",
	["mp4"] = "video.png",
	["nim"] = "nim.png",
	["otf"] = "font.png",
	["patch"] = "git.png",
	["pdf"] = "pdf.png",
	["php"] = "php.png",
	["pl"] = "prolog.png",
	["plist"] = "xml.png",
	["png"] = "image.png",
	["prisma"] = "prisma.png",
	["properties"] = "settings.png",
	["proto"] = "proto.png",
	["ps1"] = "powershell.png",
	["py"] = "python.png",
	["r"] = "r.png",
	["rb"] = "ruby.png",
	["rmd"] = "r.png",
	["rs"] = "rust.png",
	["rst"] = "markdown.png",
	["s"] = "assembly.png",
	["sass"] = "sass.png",
	["scala"] = "scala.png",
	["scss"] = "sass.png",
	["sh"] = "console.png",
	["sol"] = "solidity.png",
	["sql"] = "database.png",
	["storyboard"] = "xml.png",
	["svelte"] = "svelte.png",
	["svg"] = "svg.png",
	["swift"] = "swift.png",
	["tar"] = "zip.png",
	["tex"] = "tex.png",
	["toml"] = "toml.png",
	["ts"] = "typescript.png",
	["tsv"] = "table.png",
	["tsx"] = "react_ts.png",
	["ttf"] = "font.png",
	["txt"] = "document.png",
	["v"] = "vlang.png",
	["vb"] = "visualstudio.png",
	["vue"] = "vue.png",
	["wasm"] = "webassembly.png",
	["wav"] = "audio.png",
	["webp"] = "image.png",
	["woff"] = "font.png",
	["woff2"] = "font.png",
	["xib"] = "xml.png",
	["xml"] = "xml.png",
	["yaml"] = "yaml.png",
	["yml"] = "yaml.png",
	["zig"] = "zig.png",
	["zip"] = "zip.png",
	["zsh"] = "console.png",
}

local MATERIAL_FILES = {
	[".editorconfig"] = "editorconfig.png",
	[".env.local"] = "tune.png",
	[".eslintrc"] = "eslint.png",
	[".gitattributes"] = "git.png",
	[".gitignore"] = "git.png",
	[".gitmodules"] = "git.png",
	[".markdownlint.json"] = "markdownlint.png",
	[".npmrc"] = "npm.png",
	[".nvmrc"] = "nodejs.png",
	[".prettierrc"] = "prettier.png",
	[".swiftformat"] = "swift.png",
	["agents.md"] = "agent.png",
	["changelog.md"] = "changelog.png",
	["docker-compose.yml"] = "docker.png",
	["dockerfile"] = "docker.png",
	["gemfile"] = "gemfile.png",
	["go.mod"] = "go-mod.png",
	["go.sum"] = "go-mod.png",
	["jsconfig.json"] = "jsconfig.png",
	["justfile"] = "just.png",
	["license"] = "license.png",
	["license.md"] = "license.png",
	["makefile"] = "makefile.png",
	["next.config.js"] = "next.png",
	["package-lock.json"] = "nodejs.png",
	["package.json"] = "nodejs.png",
	["pnpm-lock.yaml"] = "pnpm.png",
	["podfile"] = "ruby.png",
	["postcss.config.js"] = "postcss.png",
	["pyproject.toml"] = "python-misc.png",
	["readme"] = "readme.png",
	["readme.md"] = "readme.png",
	["requirements.txt"] = "python-misc.png",
	["rollup.config.js"] = "rollup.png",
	["tailwind.config.js"] = "tailwindcss.png",
	["tsconfig.json"] = "tsconfig.png",
	["vite.config.ts"] = "vite.png",
	["webpack.config.js"] = "webpack.png",
	["yarn.lock"] = "yarn.png",
}

local function material_icon(path: string, material: any): string?
	if not material then
		return nil
	end
	local lower = string.lower(path)
	local base = string.match(lower, "([^/]+)$") or lower
	local icon = MATERIAL_FILES[base]
	if not icon then
		local ext = string.match(base, "%.([^.]+)$")
		icon = ext and MATERIAL_EXT[ext] or nil
	end
	return if icon and material.icons[icon] then `{material.dir}/{icon}` else nil
end

-- A nil name draws the project root; other names use the theme's aliases.
local function material_folder_icon(name: string?, open: boolean, material: any): string?
	local folders = material and material.folders
	if not folders then
		return nil
	end
	local index = if open then 2 else 1
	local pair = if name then folders.styles[folders.names[string.lower(name)]] else folders.root
	local icon = (pair and pair[index]) or folders.default[index]
	return if icon then `{material.dir}/{icon}` else nil
end

local function material_image(src: string?)
	return if src then ui.image { src = src, fit = "contain", width = 16, height = 16 } else false
end

-- The checkout's entry in `app.gitcounts`: by the repository key first, then
-- by its folder label, as the title bar's `git_pending` does. A linked
-- worktree's key can differ from the key the provider published.
local function counts_for(focused: any, root: string?, gitcounts: any): any
	local all = gitcounts and gitcounts.counts
	if type(all) ~= "table" then
		return nil
	end
	local best, icon
	local function consider(value: any)
		if type(value) ~= "table" then
			return
		end
		if best == nil then
			best = value
		end
		-- The watched checkout carries the changed files; an alias of the
		-- same repository may not. Prefer the entry that has them.
		if type(value.changes) == "table" and #value.changes > 0 then
			best = value
		end
		if icon == nil and type(value.icon) == "string" then
			icon = value.icon
		end
	end
	if root then
		consider(all[root])
	end
	if type(focused.label) == "string" then
		for key, value in all do
			if type(key) == "string" and string.match(key, "([^/]+)$") == focused.label then
				consider(value)
			end
		end
	end
	if best == nil then
		return nil
	end
	if best.icon == nil and icon ~= nil then
		local merged = {}
		for key, value in best do
			merged[key] = value
		end
		merged.icon = icon
		return merged
	end
	return best
end

local function status_color(status: string, theme: any): string
	if status == "??" then
		return theme.accent or theme.muted
	elseif string.find(status, "D", 1, true) then
		return theme.danger
	elseif string.find(status, "A", 1, true) then
		return theme.success
	elseif string.find(status, "R", 1, true) then
		return theme.warning
	end
	return theme.warning
end

return mod {
	-- The focused worktree's repository (sheprd), what gitcounts reads for it
	-- (its uncommitted lines, changed files and icon), and its commit graph.
	subscribe = { "provider/sheprd", "provider/gitcounts", "provider/gitgraph", "provider/files", "provider/material" },

	styles = {
		[".details-tab"] = { padding = { 5, 10 }, radius = 6, color = "muted-foreground" },
		[".details-tab:hover"] = { background = "list-hover", color = "foreground" },
		[".details-tab-selected"] = { background = "accent/15", color = "foreground" },
		[".details-file-row"] = { padding = { 2, 6 }, radius = 4, gap = 4 },
		[".details-file-row:hover"] = { background = "list-hover" },
		[".details-file-selected"] = { background = "accent/15" },
		[".details-file-ignored"] = { opacity = 0.5 },
		[".details-file-root"] = { padding = { 4, 6 }, radius = 4, gap = 4 },
		[".details-file-root:hover"] = { background = "list-hover" },
	},

	init = function(previous)
		local state = previous or {}
		if type(state.root) ~= "string" then
			state.root = nil
		end
		state.tab = state.tab or "git"
		state.files_open = if type(state.files_open) == "table" then state.files_open else {}
		state.files_collapsed = state.files_collapsed == true
		if type(state.files_selected) ~= "string" then
			state.files_selected = nil
		end
		-- The providers follow the window's selected workspace themselves;
		-- this state only remembers what the pane last showed. Tell them
		-- again after a reload, when they have just restarted and forgotten
		-- which checkout to read the changed files of.
		local sends = {}
		if type(state.workspace) == "string" and type(state.label) == "string" then
			table.insert(sends, { to = "provider:gitcounts", msg = { workspace = state.workspace, label = state.label } })
			table.insert(sends, { to = "provider:gitgraph", msg = { workspace = state.workspace, label = state.label } })
		end
		-- Folders the Files tab had open before a reload are listed again.
		if next(state.files_open) then
			table.insert(sends, { to = "provider:files", msg = files_open_message(state) })
		end
		if #sends > 0 then
			return state, sends
		end
		return state
	end,

	update = function(state, msg)
		if type(msg) ~= "table" then
			return
		end
		-- The Files / Git / Agents strip and the Files tree.
		if msg.select then
			state.tab = msg.select
			return
		elseif msg.toggle_file then
			state.files_open[msg.toggle_file] = if state.files_open[msg.toggle_file] then nil else true
			return state, { { to = "provider:files", msg = files_open_message(state) } }
		elseif msg.collapse_files ~= nil then
			state.files_collapsed = not state.files_collapsed
			return
		elseif msg.open_file then
			state.files_selected = msg.open_file
			return state, {
				{ to = "editor", msg = { open = msg.open_file, root = msg.root } },
				{ to = "inspector", msg = { select = msg.open_file } },
			}
		end
		if msg.topic ~= "provider/sheprd" then
			return
		end
		if type(msg.value) ~= "table" then
			return
		end
		local focused
		for _, workspace in msg.value.workspaces or {} do
			if type(workspace) == "table" and workspace.focused then
				focused = workspace
			end
		end
		if not focused or type(focused.id) ~= "string" then
			return
		end
		-- The checkout behind the workspace, for this frame's counts.
		if type(focused.repo_key) == "string" then
			state.root = string.match(focused.repo_key, "^(.*)/%.git$") or focused.repo_key
		end
		-- Tell the counts provider on every publish, not only when the
		-- workspace changes: another mod can name a different checkout in
		-- between, and the details pane would otherwise keep a stale entry.
		state.workspace = focused.id
		state.label = focused.label
		return state, {
			{ to = "provider:gitcounts", msg = { workspace = focused.id, label = focused.label } },
			{ to = "provider:gitgraph", msg = { workspace = focused.id, label = focused.label } },
		}
	end,

	view = function(state, app)
		local sheprd = app.sheprd or { workspaces = {}, agents = {} }
		local theme = app.sheprd_theme or {}
		local material = app.material
		local focused
		for _, workspace in sheprd.workspaces do
			if workspace.focused then
				focused = workspace
			end
		end
		if not focused then
			return ui.column {
				class = "sheprd-right",
				grow = true,
				padding = "normal",
				ui.text { "No worktree in focus", tone = "muted" },
			}
		end
		-- The checkout behind the focused worktree, whose uncommitted lines,
		-- changed files and icon the gitcounts provider reads.
		local root = nil
		if type(focused.repo_key) == "string" then
			root = string.match(focused.repo_key, "^(.*)/%.git$") or focused.repo_key
		end
		-- A folder opened as plain workspaces has no repository key; the
		-- files provider names the checkout the focused workspace is on.
		if not root then
			local files = app.files
			if files and files.available and type(files.path) == "string" then
				root = files.path
			end
		end
		local local_counts = counts_for(focused, root, app.gitcounts)
		-- The counts provider probes every couple of seconds; the sidebar's
		-- dirty bit refreshes more slowly, so prefer the fresher one.
		local changes = (local_counts and local_counts.changes) or {}
		local is_dirty = (local_counts and local_counts.dirty) or focused.dirty or #changes > 0
		local graph = app.gitgraph
		local commits = graph and graph.commits or {}
		local github = graph and type(graph.github) == "string" and graph.github ~= "" and graph.github or nil

		local agent_rows = {}
		for _, agent in sheprd.agents do
			-- Agents come from every listed device; the focused worktree is the
			-- selected device's, and another may use the same workspace id.
			if agent.workspace == focused.id and agent.endpoint == focused.endpoint then
				table.insert(agent_rows, ui.row {
					class = "details-agent",
					align = "center",
					gap = "small",
					msg = { focus_pane = agent.pane, endpoint = agent.endpoint },
					to = "provider:sheprd",
					status_dot { status = agent.status, theme = theme },
					ui.text { agent.name or "agent", truncate = true },
					ui.spacer(),
					ui.badge { agent.status },
				})
			end
		end

		-- One row per changed file: its Material icon, its status letter, its
		-- path, and the lines it adds and removes. Clicking reviews them.
		local change_rows = {}
		for _, change in changes do
			local icon = material_icon(change.path, material)
			table.insert(change_rows, ui.row {
				class = "details-change-row",
				align = "center",
				gap = "small",
				style = { padding = { x = 8 }, height = 24 },
				msg = { command = "review_changes" },
				to = "provider:sheprd",
				tooltip = change.path,
				material_image(icon) or ui.icon { name = "file", size = "small", style = { color = theme.muted } },
				ui.text {
					change.status,
					size = "small",
					style = { color = status_color(change.status, theme) },
				},
				ui.text {
					change.path,
					truncate = true,
					style = { color = theme.foreground },
				},
				ui.spacer(),
				if change.added and change.added ~= "0"
					then ui.text { `+{change.added}`, size = "small", style = { color = theme.added or theme.success } }
					else false,
				if change.removed and change.removed ~= "0"
					then ui.text { `-{change.removed}`, size = "small", style = { color = theme.removed or theme.danger } }
					else false,
			})
		end

		local changes_pane = ui.column {
			class = "details-changes-pane",
			grow = true,
			gap = "none",
			style = { background = theme.sidebar },
			ui.row {
				class = "details-changes-head",
				align = "center",
				style = { padding = { x = 8, y = 4 } },
				ui.text { "Changes", weight = "semibold" },
				if #changes > 0
					then ui.text { `{#changes}`, size = "small", tone = "muted" }
					else false,
				ui.spacer(),
				if root then ui.row {
					class = "details-reveal",
					align = "center",
					gap = "small",
					msg = { reveal = root },
					to = "provider:files",
					tooltip = "Reveal in Finder",
					ui.icon { name = "folder-open", size = "small", style = { color = theme.muted } },
					ui.text { "Reveal", size = "small", tone = "muted" },
				} else false,
			},
			-- The branch sits with the changes, at the top of the tab, as the
			-- classic details column shows it.
			ui.row {
				class = "details-branch-row",
				align = "center",
				gap = "small",
				style = { padding = { x = 8 }, height = 22 },
				ui.text {
					if type(focused.branch) == "string" and focused.branch ~= "" then focused.branch else "No branch",
					size = "small",
					tone = "muted",
					truncate = true,
				},
				ui.spacer(),
				if local_counts and (local_counts.added or 0) > 0
					then ui.text { `+{local_counts.added}`, size = "small", style = { color = theme.added or theme.success } }
					else false,
				if local_counts and (local_counts.removed or 0) > 0
					then ui.text { `-{local_counts.removed}`, size = "small", style = { color = theme.removed or theme.danger } }
					else false,
				if is_dirty then ui.badge { "uncommitted" } else false,
			},
			if #change_rows == 0
				then ui.text {
					if is_dirty then "Reading the changes…" else "No uncommitted changes",
					tone = "muted",
					style = { padding = { x = 8, y = 6 } },
				}
				else ui.column {
					class = "details-change-list",
					grow = true,
					scroll = true,
					gap = "none",
					table.unpack(change_rows),
				},
		}

		local history = ui.column {
			class = "details-history-pane",
			grow = true,
			gap = "small",
			padding = "normal",
			style = { background = theme.sidebar },
			ui.row {
				class = "details-history-head",
				align = "center",
				ui.text { "History", weight = "semibold" },
				ui.spacer(),
				if github then ui.row {
					class = "details-github",
					align = "center",
					gap = "small",
					msg = github,
					to = "app:open",
					tooltip = "View on GitHub",
					ui.icon { name = "brand-github", size = "small", style = { color = theme.muted } },
					ui.text { "GitHub", size = "small", tone = "muted" },
				} else false,
				if #commits > 0
					then ui.text { `{#commits}`, size = "small", tone = "muted" }
					else false,
			},
			if #commits > 0 then ui.column {
				class = "details-history",
				grow = true,
				scroll = true,
				commit_graph { commits = commits, compact = true, row_height = 22 },
			} elseif graph and graph.repository == false then ui.text {
				"No repository",
				tone = "muted",
			} else ui.text {
				"Reading commits…",
				tone = "muted",
			},
		}

		-- The Files tab: the same provider the Explorer tab reads, in the
		-- details column. Folders the tab opens are listed by the provider.
		local files = app.files
		local files_pane
		if not files then
			files_pane = ui.column {
				class = "details-files-pane",
				grow = true,
				padding = "normal",
				ui.text { "Loading files…", tone = "muted" },
			}
		elseif not files.available then
			files_pane = ui.column {
				class = "details-files-pane",
				grow = true,
				padding = "normal",
				ui.text { files.reason or "No files to show.", tone = "muted" },
			}
		else
			local lines = {}
			local function add(dir: string, depth: number)
				local entries = files.dirs and files.dirs[dir]
				if not entries then
					table.insert(lines, { path = dir, depth = depth })
					return
				end
				for _, entry in entries do
					local path = if dir == "" then entry.name else `{dir}/{entry.name}`
					local open = entry.dir and state.files_open[path] or false
					table.insert(lines, { path = path, depth = depth, entry = entry, open = open })
					if open then
						add(path, depth + 1)
					end
				end
			end
			local function file_line(at: any)
				if not at.entry then
					return ui.row {
						class = "details-file-row",
						ui.row { width = INDENT * at.depth + 16 },
						ui.text { "…", tone = "muted", size = "small" },
					}
				end
				local classes = { "details-file-row" }
				if at.path == state.files_selected then
					table.insert(classes, "details-file-selected")
				end
				if at.entry.ignored then
					table.insert(classes, "details-file-ignored")
				end
				local lead, image, fallback
				if at.entry.dir then
					image = material_folder_icon(at.entry.name, at.open, material)
					fallback = if at.open then "folder-open" else "folder"
					lead = ui.icon { name = if at.open then "chevron-down" else "chevron-right", size = "small" }
				else
					image = material_icon(at.path, material)
					fallback = "file-text"
					lead = ui.row { width = 14 }
				end
				return ui.row {
					class = classes,
					align = "center",
					msg = if at.entry.dir
						then { toggle_file = at.path }
						else { open_file = at.path, root = files.path },
					ui.row { width = INDENT * at.depth },
					lead,
					material_image(image) or ui.icon { name = fallback, size = "small" },
					ui.text { at.entry.name, truncate = true },
				}
			end
			local root_row = ui.row {
				class = "details-file-root",
				align = "center",
				msg = { collapse_files = true },
				ui.icon {
					name = if state.files_collapsed then "chevron-right" else "chevron-down",
					size = "small",
				},
				material_image(material_folder_icon(nil, not state.files_collapsed, material)),
				ui.text { string.upper(files.root or "Files"), weight = "bold", size = "small" },
			}
			if state.files_collapsed then
				files_pane = ui.column {
					class = "details-files-pane",
					grow = true,
					gap = "none",
					padding = "small",
					root_row,
				}
			else
				add("", 0)
				files_pane = ui.column {
					class = "details-files-pane",
					grow = true,
					gap = "none",
					padding = "small",
					root_row,
					ui.list {
						id = "details-files",
						grow = true,
						count = #lines,
						row_height = ROW_HEIGHT,
						row = function(index)
							return file_line(lines[index])
						end,
					},
				}
			end
		end

		local agents_pane = ui.column {
			class = "details-agents-pane",
			grow = true,
			gap = "small",
			padding = "normal",
			style = { background = theme.sidebar },
			ui.row {
				class = "details-agents-head",
				align = "center",
				ui.text { "Agents", weight = "semibold" },
				ui.spacer(),
				ui.text { tostring(#agent_rows), size = "small", tone = "muted" },
			},
			if #agent_rows > 0
				then ui.column { grow = true, scroll = true, gap = "small", table.unpack(agent_rows) }
				else ui.text { "None here", tone = "muted" },
		}

		local git_pane = ui.column {
			class = "details-git-pane",
			grow = true,
			gap = "none",
			ui.vsplit {
				id = "details-git-split",
				ui.pane {
					class = "details-changes-split",
					changes_pane,
				},
				ui.pane {
					class = "details-history-split",
					size = 220,
					min = 100,
					max = 480,
					fixed = true,
					history,
				},
			},
		}

		local active = if state.tab == "files"
			then files_pane
			elseif state.tab == "agents" then agents_pane
			else git_pane

		return ui.column {
			class = "details-column",
			grow = true,
			gap = "none",
			style = { background = theme.sidebar },
			ui.row {
				class = "details-tabs-bar",
				shrink = false,
				gap = "small",
				style = { background = theme.sidebar, padding = { x = 6, y = 4 } },
				tab_button(state, "files", "Files", "folder"),
				tab_button(state, "git", "Git", "git-branch"),
				tab_button(state, "agents", "Agents", "sparkles"),
			},
			ui.column {
				class = "details-tab-body",
				grow = true,
				gap = "none",
				active,
			},
			-- The project's own icon, at the column's foot, when it has one.
			-- The row paints the theme's sidebar behind the image, so the
			-- letterbox around a non-square icon is not black.
			if local_counts and local_counts.icon
				then ui.row {
					class = "details-icon",
					justify = "center",
					align = "center",
					shrink = false,
					style = { background = theme.sidebar, padding = { x = 12, y = 8 } },
					ui.image {
						src = local_counts.icon,
						fit = "contain",
						width = 276,
						height = 96,
						class = "details-icon-image",
					},
				}
				else false,
		}
	end,
}
