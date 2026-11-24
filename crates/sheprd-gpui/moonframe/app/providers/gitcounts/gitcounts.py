#!/usr/bin/env python3
"""Line counts and project icons for every workspace Herdr lists.

Prints one JSON line, `{ "counts": { "<checkout>": { "added", "removed",
"dirty", "icon"? } } }`, whenever they change. The checkouts come from
Herdr's session snapshots (like the gitgraph provider), and a mod may also
name some on stdin: `{ "roots": ["<path>", …] }`. Exits when the app closes
stdin.

Tracked changes against HEAD are summed (staged and unstaged); untracked
files make the checkout dirty but count no line, as Sheprd's own bar does.
A checkout's `icon.png` is copied into the app's image cache, the only
folder a mod may show pictures from.
"""

import glob
import hashlib
import json
import os
import select
import shutil
import subprocess
import sys
import time
from pathlib import Path

POLL = 3.0
TIMEOUT = 3

CACHE = Path(os.environ.get("XDG_CACHE_HOME") or os.path.expanduser("~/.cache")) / "sheprd" / "images"
COPIED = {}
DEBUG = Path("/tmp/gitcounts-debug.log")


def debug(line: str) -> None:
    try:
        with DEBUG.open("a") as log:
            log.write(f"{time.time():.0f} {line}\n")
    except Exception:
        pass


def git(root: str, *args: str) -> str:
    try:
        done = subprocess.run(
            ["git", "-C", root, *args],
            capture_output=True,
            text=True,
            timeout=TIMEOUT,
        )
        return done.stdout
    except Exception:
        return ""


def icon_for(root: str):
    """The project's icon: its checkout's, else the repository's or the main
    worktree's. A linked worktree often has no icon of its own, while the
    checkout it was made from does."""
    try:
        candidates = [Path(root) / "icon.png"]
        top = git(root, "rev-parse", "--show-toplevel").strip()
        if top and Path(top) != Path(root):
            candidates.append(Path(top) / "icon.png")
        common = git(root, "rev-parse", "--git-common-dir").strip()
        if common:
            main = Path(os.path.abspath(os.path.join(root, common))).parent
            candidates.append(main / "icon.png")
        source = next((path for path in candidates if path.is_file()), None)
        if source is None:
            return None
        stat = source.stat()
        key = (stat.st_mtime_ns, stat.st_size)
        CACHE.mkdir(parents=True, exist_ok=True)
        dest = CACHE / f"gitcounts-{hashlib.sha1(str(source).encode()).hexdigest()[:12]}{source.suffix}"
        if COPIED.get(str(source)) != key or not dest.is_file():
            shutil.copyfile(source, dest)
            COPIED[str(source)] = key
        return str(dest)
    except Exception:
        return None


def counts(root: str):
    try:
        diff = git(root, "diff", "HEAD", "--numstat")
        added = removed = 0
        for line in diff.splitlines():
            parts = line.split("\t")
            if len(parts) < 2:
                continue
            if parts[0].isdigit():
                added += int(parts[0])
            if parts[1].isdigit():
                removed += int(parts[1])
        # Untracked folders (node_modules…) make the scan crawl; tracked
        # changes carry the lines, so the dirty bit follows them.
        status = git(root, "status", "--porcelain", "--untracked-files=no")
        found = {
            "added": added,
            "removed": removed,
            "dirty": bool(status.strip()),
        }
        icon = icon_for(root)
        if icon is not None:
            found["icon"] = icon
        return found
    except Exception:
        return None


def focused_checkout():
    """The focused workspace's checkout, as the running app reports it."""
    import glob as _glob
    import shutil as _shutil
    import subprocess as _subprocess
    home = os.path.expanduser("~")
    def snapshot_cwd(wsid):
        snapshots = _glob.glob(home + "/.config/herdr/session-snapshots/*.json")
        snapshots.sort(key=os.path.getmtime, reverse=True)
        for path in snapshots[:12]:
            try:
                state = json.load(open(path))
            except Exception:
                continue
            for workspace in state.get("workspaces") or []:
                if workspace.get("id") != wsid:
                    continue
                cwd = workspace.get("identity_cwd")
                if cwd:
                    return cwd
        return None
    for exe in (_shutil.which("sheprd"), "/opt/homebrew/bin/sheprd",
                home + "/.local/bin/sheprd",
                "/Applications/Sheprd.app/Contents/MacOS/sheprd",
                home + "/Applications/Sheprd Groupes.app/Contents/MacOS/sheprd"):
        if not exe or not os.path.exists(exe):
            continue
        try:
            data = json.loads(_subprocess.run([exe, "status", "--json"],
                capture_output=True, text=True, timeout=2).stdout)
        except Exception:
            continue
        wsid = None
        for window in data.get("windows") or []:
            if window.get("active"):
                wsid = window.get("workspace")
                break
        if not wsid:
            wsid = next((w.get("workspace") for w in data.get("windows") or []), None)
        if wsid:
            cwd = snapshot_cwd(wsid)
            if cwd:
                return cwd
    return None


def workspace_cwd(wsid: str):
    """The checkout of a sidebar workspace, from Herdr's snapshots."""
    home = os.path.expanduser("~")
    snapshots = glob.glob(home + "/.config/herdr/session-snapshots/*.json")
    snapshots.sort(key=os.path.getmtime, reverse=True)
    for path in snapshots[:12]:
        try:
            with open(path) as handle:
                state = json.load(handle)
        except Exception:
            continue
        for workspace in state.get("workspaces") or []:
            if workspace.get("id") != wsid:
                continue
            cwd = workspace.get("identity_cwd")
            if not cwd:
                for tab in workspace.get("tabs") or []:
                    for pane in (tab.get("panes") or {}).values():
                        if pane.get("cwd"):
                            cwd = pane["cwd"]
                            break
                    if cwd:
                        break
            if cwd:
                return cwd
    return None


def changes_of(root: str):
    """The checkout's changed files: status, path, and diff lines."""
    try:
        numstat = {}
        for line in git(root, "diff", "HEAD", "--numstat").splitlines():
            parts = line.split("\t")
            if len(parts) >= 3:
                numstat[parts[2]] = (parts[0], parts[1])
        entries = []
        for line in git(root, "status", "--porcelain").splitlines():
            if len(line) < 4:
                continue
            status = line[:2].strip() or "?"
            path = line[3:]
            added, removed = numstat.get(path, ("0", "0"))
            entries.append({"path": path, "status": status,
                            "added": added, "removed": removed})
        return entries[:200]
    except Exception:
        return []


def workspace_paths():
    """Every sidebar workspace's checkout, keyed by its Herdr id. A mod
    without a `repo_key` (most rows) groups its projects from this."""
    paths = {}
    home = os.path.expanduser("~")
    snapshots = glob.glob(home + "/.config/herdr/session-snapshots/*.json")
    snapshots.sort(key=os.path.getmtime, reverse=True)
    for path in snapshots[:12]:
        try:
            with open(path) as handle:
                state = json.load(handle)
        except Exception:
            continue
        for workspace in state.get("workspaces") or []:
            wsid = workspace.get("id")
            cwd = workspace.get("identity_cwd")
            if not cwd:
                for tab in workspace.get("tabs") or []:
                    for pane in (tab.get("panes") or {}).values():
                        if pane.get("cwd"):
                            cwd = pane["cwd"]
                            break
                    if cwd:
                        break
            if wsid and cwd and wsid not in paths:
                paths[wsid] = cwd
    return paths


def workspace_roots():
    """Every workspace's checkout, from Herdr's newest session snapshots."""
    roots = []
    home = os.path.expanduser("~")
    snapshots = glob.glob(home + "/.config/herdr/session-snapshots/*.json")
    snapshots.sort(key=os.path.getmtime, reverse=True)
    for path in snapshots[:12]:
        try:
            with open(path) as handle:
                state = json.load(handle)
        except Exception:
            continue
        for workspace in state.get("workspaces") or []:
            cwd = workspace.get("identity_cwd")
            if not cwd:
                for tab in workspace.get("tabs") or []:
                    for pane in (tab.get("panes") or {}).values():
                        if pane.get("cwd"):
                            cwd = pane["cwd"]
                            break
                    if cwd:
                        break
            if cwd and cwd not in roots:
                roots.append(cwd)
    return roots


def named_roots(line: str):
    """The checkouts a message names: a mod's roots, or the app's project."""
    try:
        msg = json.loads(line)
    except Exception:
        msg = None
    if isinstance(msg, dict):
        roots = msg.get("roots")
        if isinstance(roots, list):
            return [path for path in roots if isinstance(path, str)]
        if isinstance(msg.get("workspace"), str):
            cwd = workspace_cwd(msg["workspace"])
            return [cwd] if cwd else []
        if isinstance(msg.get("project"), str):
            return [msg["project"]]
        inner = msg.get("message") or msg.get("msg")
        if isinstance(inner, dict):
            return named_roots(json.dumps(inner))
    marker = '"project":"'
    if marker in line:
        return [line.split(marker, 1)[1].split('"', 1)[0]]
    return None


KEY_CACHE = {}
SKIP = set()


def keys_for(root: str):
    """The names a checkout answers under: its own path, its repository root,
    and the `.git` folder's parent (the sidebar's repo key), cached."""
    if root in KEY_CACHE:
        return KEY_CACHE[root]
    keys = {root}
    top = git(root, "rev-parse", "--show-toplevel").strip()
    if not top:
        SKIP.add(root)
        return None
    keys.add(top)
    common = git(root, "rev-parse", "--git-common-dir").strip()
    if common:
        keys.add(os.path.dirname(os.path.abspath(os.path.join(root, common))))
    KEY_CACHE[root] = keys
    return keys


def main() -> None:
    extra = []
    focus = focused_checkout()
    data = {}
    known = []
    queue = []
    last = None
    # Publish at once, so the app keeps a provider that answers quickly; the
    # checkouts fill in a few at a time after that.
    sys.stdout.write(json.dumps({"counts": {}}) + "\n")
    sys.stdout.flush()
    while True:
        ready, _, _ = select.select([sys.stdin], [], [], POLL)
        if ready:
            line = sys.stdin.readline()
            if line == "":
                return
            named = named_roots(line)
            if named is not None:
                # Keep the checkouts the mods named watching, not just the
                # newest: the shell and the details pane name different ones
                # in turn, and a later message must not drop the other's
                # changed files.
                for root in named:
                    if root and root not in extra:
                        extra.append(root)
                extra = extra[-8:]
        live = focused_checkout()
        if live:
            focus = live
        roots = list(extra)
        for root in workspace_roots():
            if root not in roots:
                roots.append(root)
        if roots != known:
            known = roots
            queue = [root for root in roots if root not in SKIP]
        # The focused checkout stays fresh; the rest fill in, a few per poll.
        for root in extra:
            if root not in SKIP:
                queue = [root] + [item for item in queue if item != root]
        # The checkouts whose changed files are read: the mod names one, and
        # the CLI's focus is a fallback. Every other checkout of the same
        # repository (a worktree) answers under the same repository keys, so
        # it must not overwrite the entry that carries the files.
        watched = [root for root in [*extra, focus] if root]
        watched_keys = set()
        for root in watched:
            keys = keys_for(root)
            if keys:
                watched_keys |= keys
        for _ in range(min(12, len(queue))):
            root = queue.pop(0)
            if root in SKIP:
                continue
            keys = keys_for(root)
            if keys is None:
                continue
            found = counts(root)
            if found is not None:
                # The mod names the checkout it shows; follow that, and the
                # CLI's focus as a fallback.
                if root in watched:
                    found["changes"] = changes_of(root)
                    found["dirty"] = bool(found["changes"])
                    debug(f"changes {root}: {len(found['changes'])}")
                for key in keys:
                    if root not in watched and key in watched_keys:
                        continue
                    data[key] = found
        if not queue:
            queue = [root for root in known if root not in SKIP]
        debug(f"tick extra={extra} roots={len(roots)} queue={len(queue)} data={len(data)} skip={len(SKIP)}")
        # `data` is mutated in place, so comparing it with a dict saved from
        # the last publish always says "equal": serialize the snapshot.
        payload = json.dumps({"counts": data, "workspaces": workspace_paths()})
        if payload != last:
            sys.stdout.write(payload + "\n")
            sys.stdout.flush()
            last = payload
            debug(f"publish {len(data)} keys")


if __name__ == "__main__":
    main()
