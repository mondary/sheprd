#!/usr/bin/env bash
# Prints the focused repository's recent commits as one JSON line, for the
# right pane's graph: when they change (looked at every few seconds) and at
# once when a mod sends a message. Exits when the app closes its stdin.
#
# Output: { repository, root, path, branch, github, commits = { { sha,
#   short, parents, subject, author, refs } } }, children before parents, as
#   `git log` gives them. `github` is the repository's page on github.com,
#   or "" when its remote is not GitHub's.
#
# The repository is the project the user works in (MOONFRAME_PROJECT, then
# `{ project = "<path>" }` from the app as it moves), or the folder a mod
# names with `{ root = "<path>" }`: the focused worktree's. Plain bash (3.2
# or later): every string goes through `jstr`.

POLL=3  # seconds between looks
MAX=80  # commits kept

APP_PROJECT=${MOONFRAME_PROJECT:-}
WANTED=
PROJECT=${WANTED:-${APP_PROJECT:-$PWD}}

# Sets J to $1 as a JSON string, quotes included. No subshell: it runs for
# every field of every commit.
jstr() {
    local s=$1
    s=${s//\\/\\\\}
    s=${s//\"/\\\"}
    s=${s//$'\t'/\\t}
    s=${s//$'\n'/\\n}
    s=${s//$'\r'/}
    J="\"$s\""
}

# The JSON array of a commit's refs (`main, origin/main, tag: v1`): `HEAD ->`
# and `tag:` dropped, and `origin/HEAD`, which says nothing.
refs_json() {
    local list=$1 item name out= sep=
    while [ -n "$list" ]; do
        item=${list%%, *}
        case $list in
            *,*) list=${list#*, } ;;
            *) list= ;;
        esac
        case $item in
            'HEAD -> '*) name=${item#'HEAD -> '} ;;
            'tag: '*) name=${item#'tag: '} ;;
            *) name=$item ;;
        esac
        case $name in
            '' | 'origin/HEAD') continue ;;
        esac
        jstr "$name"
        out="$out$sep$J"
        sep=,
    done
    printf '[%s]' "$out"
}

# The repository's page on github.com, from any of the remote's URL forms,
# or nothing when its remote is elsewhere.
github_url() {
    local url=$1 path
    case $url in
        git@github.com:*) path=${url#git@github.com:} ;;
        ssh://git@github.com/*) path=${url#ssh://git@github.com/} ;;
        https://github.com/* | http://github.com/*) path=${url#*github.com/} ;;
        git://github.com/*) path=${url#git://github.com/} ;;
        *) return 0 ;;
    esac
    printf 'https://github.com/%s' "${path%.git}"
}

# The whole publication, as one line without its newline.
build() {
    local top branch commits= sep= sha short parents subject author refs
    local one plist ps root remote web=
    if ! top=$(git -C "$PROJECT" rev-parse --show-toplevel 2>/dev/null); then
        printf '{"repository":false}'
        return
    fi
    branch=$(git -C "$PROJECT" rev-parse --abbrev-ref HEAD 2>/dev/null) || branch=HEAD
    remote=$(git -C "$PROJECT" remote 2>/dev/null | head -n1)
    [ -n "$remote" ] && web=$(github_url "$(git -C "$PROJECT" remote get-url "$remote" 2>/dev/null)")
    while IFS=$'\x1f' read -r sha short parents subject author refs; do
        [ -n "$sha" ] || continue
        plist= ps=
        for p in $parents; do
            jstr "$p"
            plist="$plist$ps$J"
            ps=,
        done
        jstr "$sha"
        one="{\"sha\":$J"
        jstr "$short"
        one="$one,\"short\":$J"
        one="$one,\"parents\":[$plist]"
        jstr "$subject"
        one="$one,\"subject\":$J"
        jstr "$author"
        one="$one,\"author\":$J"
        one="$one,\"refs\":$(refs_json "$refs")}"
        commits="$commits$sep$one"
        sep=,
    done < <(git -C "$PROJECT" log --max-count="$MAX" --pretty=tformat:'%H%x1f%h%x1f%P%x1f%s%x1f%an%x1f%D' 2>/dev/null)
    jstr "$top"
    local path=$J
    root=$(basename "$top")
    jstr "$root"
    local name=$J
    jstr "$branch"
    local branch_json=$J
    jstr "$web"
    printf '{"repository":true,"root":%s,"path":%s,"branch":%s,"github":%s,"commits":[%s]}' "$name" "$path" "$branch_json" "$J" "$commits"
}

# The checkout of a sidebar workspace: Herdr keeps each workspace's directory
# in its latest session snapshots, by the id the sidebar publishes. When only
# a label is left (another host, an older snapshot), the project of that name
# under the GitHub folders stands in.
workspace_cwd() {
    /usr/bin/python3 - "$1" "${2:-}" <<'PYW'
import glob, json, os, sys

wid, label = sys.argv[1], sys.argv[2]
home = os.path.expanduser("~")
found = None
snapshots = glob.glob(home + "/.config/herdr/session-snapshots/*.json")
snapshots.sort(key=os.path.getmtime, reverse=True)
for path in snapshots[:12]:
    try:
        with open(path) as f:
            state = json.load(f)
    except Exception:
        continue
    for workspace in state.get("workspaces") or []:
        if workspace.get("id") != wid:
            continue
        if not label:
            label = workspace.get("custom_name") or ""
        found = workspace.get("identity_cwd")
        if not found:
            for tab in workspace.get("tabs") or []:
                for pane in (tab.get("panes") or {}).values():
                    if pane.get("cwd"):
                        found = pane["cwd"]
                        break
                if found:
                    break
        if found:
            break
    if found:
        break

def is_repo(path):
    return os.path.exists(os.path.join(path, ".git"))

def in_repo(path):
    while path and path != "/":
        if is_repo(path):
            return True
        path = os.path.dirname(path)
    return False

candidate = found
if found and in_repo(found):
    print(found)
    sys.exit(0)

if label:
    roots = [home + "/Documents/GitHub/PROJECTS", home + "/Documents/GitHub"]
    for root in roots:
        if not os.path.isdir(root):
            continue
        base = root.rstrip("/").count("/")
        best = None
        for current, dirs, _ in os.walk(root):
            if current.count("/") - base >= 3:
                dirs[:] = []
                continue
            dirs[:] = [d for d in dirs if not d.startswith(".")]
            name = os.path.basename(current)
            if name == label and is_repo(current):
                print(current)
                sys.exit(0)
            if name.startswith(label) and is_repo(current) and best is None:
                best = current
        if best:
            print(best)
            sys.exit(0)
if candidate:
    print(candidate)
PYW
}

# The workspace the right pane last showed, kept by the details mod: the graph
# follows it from the start, as the pane does.
saved_details() {
    /usr/bin/python3 - <<'PYW'
import json, os, sqlite3
path = os.path.expanduser("~/.local/state/sheprd/state.sqlite")
details = {}
try:
    connection = sqlite3.connect("file:" + path + "?mode=ro", uri=True)
    row = connection.execute(
        "select value from state where key = 'mod:details'"
    ).fetchone()
    if row:
        details = json.loads(row[0])
except Exception:
    pass
print(details.get("workspace") or "")
print(details.get("label") or "")
PYW
}

# The workspace the window has active: its id, then the name it was given, so
# the first publication before any mod speaks already shows a graph.
active_workspace() {
    /usr/bin/python3 - <<'PYW'
import glob, json, os
home = os.path.expanduser("~")
snapshots = glob.glob(home + "/.config/herdr/session-snapshots/*.json")
snapshots.sort(key=os.path.getmtime, reverse=True)
for path in snapshots[:1]:
    try:
        with open(path) as f:
            state = json.load(f)
    except Exception:
        continue
    workspaces = state.get("workspaces") or []
    index = state.get("active")
    if isinstance(index, int) and 0 <= index < len(workspaces):
        workspace = workspaces[index]
        print(workspace.get("id", ""))
        print(workspace.get("custom_name") or "")
PYW
}

# The checkout the window shows now: the sidebar's selection, resolved
# through Herdr's snapshots. A click moves the graph without any mod message.
live_project() {
    local active id label dir
    active=$(active_workspace)
    [ -n "$active" ] || return 0
    {
        IFS= read -r id
        IFS= read -r label
    } <<EOF
$active
EOF
    [ -n "$id" ] || return 0
    dir=$(workspace_cwd "$id" "$label")
    [ -n "$dir" ] && printf '%s' "$dir"
}

# The project in a message: from the app itself (`from` empty, which no mod
# has) as `project`, from a mod as `root` (the checkout of the repository's
# `.git` path), or as `workspace`, the focused sidebar workspace's id.
project_of() {
    local line=$1 rest label dir
    case $line in
        *'"root":"'*)
            rest=${line#*'"root":"'}
            WANTED=${rest%%'"'*}
            ;;
        *'"workspace":"'*)
            rest=${line#*'"workspace":"'}
            rest=${rest%%'"'*}
            label=
            case $line in
                *'"label":"'*)
                    label=${line#*'"label":"'}
                    label=${label%%'"'*}
                    ;;
            esac
            dir=$(workspace_cwd "$rest" "$label")
            if [ -n "$dir" ]; then
                WANTED=$dir
            else
                WANTED=/nonexistent
            fi
            ;;
        *'"project":"'*)
            rest=${line#*'"project":"'}
            APP_PROJECT=${rest%%'"'*}
            ;;
    esac
    PROJECT=${WANTED:-${APP_PROJECT:-$PWD}}
}

# Before any mod speaks, follow the workspace the pane last showed; failing
# that, the one the window has active.
SAVED=$(saved_details)
ACTIVE=$(active_workspace)
if [ -n "$SAVED" ]; then
    {
        IFS= read -r SAVED_ID
        IFS= read -r SAVED_LABEL
    } <<EOF
$SAVED
EOF
    if [ -n "$SAVED_ID" ]; then
        SAVED_DIR=$(workspace_cwd "$SAVED_ID" "$SAVED_LABEL")
        if [ -n "$SAVED_DIR" ]; then
            WANTED=$SAVED_DIR
            PROJECT=$WANTED
        fi
    fi
fi
if [ -z "$WANTED" ] && [ -n "$ACTIVE" ]; then
    {
        IFS= read -r ACTIVE_ID
        IFS= read -r ACTIVE_LABEL
    } <<EOF
$ACTIVE
EOF
    ACTIVE_DIR=$(workspace_cwd "$ACTIVE_ID" "$ACTIVE_LABEL")
    if [ -n "$ACTIVE_DIR" ]; then
        WANTED=$ACTIVE_DIR
        PROJECT=$WANTED
    fi
fi

printf '%s %s\n' "$(date +%s)" "$PROJECT" >> /tmp/gitgraph-debug.log

LAST=
TOLD=
while true; do
    if [ -z "$TOLD" ]; then
        LIVE=$(live_project)
        if [ -n "$LIVE" ] && [ "$LIVE" != "$PROJECT" ]; then
            PROJECT=$LIVE
        fi
    fi
    OUT=$(build)
    if [ "$OUT" != "$LAST" ]; then
        printf '%s\n' "$OUT"
        LAST=$OUT
    fi
    # A message, or POLL quiet seconds, looks again. The app closing stdin
    # ends the script. bash 3.2 returns 1 for both a timeout and the end of
    # input, so a `read` that failed before its timeout means the end.
    started=$SECONDS
    if IFS= read -r -t "$POLL" message; then
        project_of "$message"
        TOLD=1
    else
        status=$?
        if [ "$status" -le 128 ] && [ $((SECONDS - started)) -lt "$((POLL - 1))" ]; then
            exit 0
        fi
    fi
done
