"""Read local Git checkouts for the Details pane, without changing their index."""

import json
import os
import selectors
import subprocess
import sys
import time
from collections import deque
from urllib.parse import quote

POLL = 3
SCAN_INTERVAL = 12
MAX_DIRS = 3000
MAX_DEPTH = 5
MAX_REPOS = 24
SKIP = {"node_modules", "target", "build", "dist", "DerivedData", "__pycache__"}


def git(path, *args):
    try:
        result = subprocess.run(
            ["git", "--no-optional-locks", "-C", path, *args],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=2,
            env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
        )
        if result.returncode == 0:
            return result.stdout.decode("utf-8", errors="replace")
    except (OSError, subprocess.TimeoutExpired):
        pass
    return None


def discover(scope):
    """Use an enclosing checkout or look beneath a plain project folder."""
    if not scope or not os.path.isdir(scope):
        return [], False
    path = os.path.realpath(scope)
    top = git(path, "rev-parse", "--show-toplevel")
    if top:
        return [(top.rstrip("\n"), os.path.basename(top.rstrip("\n")))], False
    found = []
    queue = deque([(path, 0)])
    visited = 0
    while queue and visited < MAX_DIRS and len(found) < MAX_REPOS:
        folder, depth = queue.popleft()
        visited += 1
        if os.path.lexists(os.path.join(folder, ".git")):
            top = git(folder, "rev-parse", "--show-toplevel")
            if top and top.rstrip("\n") == folder:
                found.append((folder, os.path.relpath(folder, path)))
                continue
        if depth == MAX_DEPTH:
            continue
        try:
            with os.scandir(folder) as entries:
                children = sorted(
                    entry.path for entry in entries
                    if not entry.name.startswith(".") and entry.name not in SKIP
                    and entry.is_dir(follow_symlinks=False)
                )
        except OSError:
            continue
        queue.extend((child, depth + 1) for child in children[:max(0, MAX_DIRS - visited)])
    return found, bool(queue)


def status(path):
    raw = git(path, "status", "--porcelain=v1", "-z", "--branch", "--untracked-files=normal", "--no-renames")
    if raw is None:
        return {"branch": "", "dirty": False, "error": True, "count": 0}, []
    header, *entries = raw.rstrip("\0").split("\0")
    branch = header.removeprefix("## ")
    if branch == "HEAD (no branch)":
        branch = "Detached HEAD"
    else:
        branch = branch.removeprefix("No commits yet on ").split("...", 1)[0]
    changes = [(entry[:2].strip(), entry[3:]) for entry in entries if len(entry) >= 4]
    return {"branch": branch, "dirty": bool(changes), "count": len(changes)}, changes


def details(path, changes):
    # -z preserves spaces, tabs and newlines in file names. Show staged files in
    # an unborn branch too, where HEAD does not exist yet.
    diff = git(path, "diff", "--numstat", "-z", "--no-renames", "HEAD", "--")
    if diff is None:
        diff = git(path, "diff", "--cached", "--numstat", "-z", "--no-renames", "--")
    stat = {}
    for entry in (diff or "").split("\0"):
        fields = entry.split("\t", 2)
        if len(fields) == 3:
            added, removed, name = fields
            stat[name] = (added, removed)
    rows = []
    for letter, name in changes[:200]:
        added, removed = stat.get(name, ("0", "0"))
        rows.append({"path": name, "status": letter, "added": added, "removed": removed})
    log = git(path, "log", "-80", "-z", "--format=%H%x00%h%x00%P%x00%s%x00%an%x00%D")
    commits = []
    fields = iter((log or "").split("\0"))
    for sha, short, parents, subject, author, refs in zip(*[fields] * 6):
        refs = [ref.removeprefix("HEAD -> ").removeprefix("tag: ")
                for ref in refs.split(", ") if ref and ref != "origin/HEAD"]
        commits.append(dict(sha=sha, short=short, parents=parents.split(), subject=subject,
                            author=author, refs=refs))
    remotes = (git(path, "remote") or "").splitlines()
    remote_name = "origin" if "origin" in remotes else next(iter(remotes), None)
    remote = (git(path, "remote", "get-url", remote_name) or "").strip() if remote_name else ""
    github = ""
    for prefix in ("git@github.com:", "ssh://git@github.com/", "https://github.com/",
                   "http://github.com/", "git://github.com/"):
        if remote.startswith(prefix):
            parts = remote[len(prefix):].removesuffix(".git").strip("/").split("/")
            if len(parts) == 2 and all(part and part not in (".", "..") for part in parts):
                github = "https://github.com/" + "/".join(quote(part, safe="") for part in parts)
            break
    return {"changes": rows, "commits": commits, "github": github,
            "added": sum(int(a) for a, _ in stat.values() if a.isdecimal()),
            "removed": sum(int(r) for _, r in stat.values() if r.isdecimal())}


def snapshot(scope, wanted, repos, limited):
    selected = wanted if any(path == wanted for path, _ in repos) else (repos[0][0] if repos else None)
    summaries = []
    active = None
    for path, name in repos:
        summary, changes = status(path)
        summaries.append({"path": path, "name": name, **summary})
        if path == selected:
            active = {**summaries[-1], **details(path, changes)}
    return {"scope": scope, "repos": summaries, "selected": active, "limited": limited}


def request(line):
    try:
        msg = json.loads(line)
    except (ValueError, UnicodeError):
        return None
    if not isinstance(msg, dict):
        return None
    msg = msg.get("message", msg.get("msg", msg))
    if not isinstance(msg, dict) or not isinstance(msg.get("git_scope"), str):
        return None
    scope = msg["git_scope"]
    if scope and (not os.path.isabs(scope) or "\0" in scope):
        return None
    return scope, msg.get("repository")


def main():
    scope, wanted, last = "", None, None
    repos, limited = [], False
    next_scan = 0
    # Use an unbuffered fd with select: several messages can arrive at once.
    with selectors.DefaultSelector() as selector:
        selector.register(sys.stdin, selectors.EVENT_READ)
        pending = b""
        while True:
            if time.monotonic() >= next_scan:
                repos, limited = discover(scope)
                next_scan = time.monotonic() + SCAN_INTERVAL
            payload = json.dumps(snapshot(scope, wanted, repos, limited))
            if payload != last:
                print(payload, flush=True)
                last = payload
            deadline = time.monotonic() + POLL
            while selector.select(max(0, deadline - time.monotonic())):
                data = os.read(sys.stdin.fileno(), 65536)
                if not data:
                    return
                pending += data
                *lines, pending = pending.split(b"\n")
                latest = next((parsed for line in reversed(lines) if (parsed := request(line)) is not None), None)
                if latest is not None and latest != (scope, wanted):
                    if latest[0] != scope:
                        next_scan = 0
                    scope, wanted = latest
                    break
                if time.monotonic() >= deadline:
                    break


if __name__ == "__main__":
    main()
