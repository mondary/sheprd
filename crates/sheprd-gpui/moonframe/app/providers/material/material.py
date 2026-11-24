#!/usr/bin/env python3
"""Publishes the available Material Icon Theme files and folder styles.

The installed pack lives under `~/.config/sheprd/material/`. Copy its
images into Moonframe's permitted cache before publishing their names.
"""

import json
import os
import select
import shutil
import sys
from functools import lru_cache
from pathlib import Path

HOME = Path(os.path.expanduser("~"))
PACK = HOME / ".config" / "sheprd" / "material"
CACHE = Path(os.environ.get("XDG_CACHE_HOME") or HOME / ".cache") / "sheprd-moonframe" / "images" / "material"


def cache_icon(icon: Path) -> bool:
    dest = CACHE / icon.name
    try:
        size = icon.stat().st_size
        if not dest.is_file() or dest.stat().st_size != size:
            shutil.copyfile(icon, dest)
    except OSError:
        return False
    return True


def sync() -> dict[str, bool]:
    try:
        CACHE.mkdir(parents=True, exist_ok=True)
    except OSError:
        return {}
    return {icon.name: True for icon in (PACK / "png").glob("*.png") if cache_icon(icon)}


def folder_theme():
    try:
        theme = json.loads((PACK / "material-icons.json").read_text(encoding="utf-8"))
        CACHE.mkdir(parents=True, exist_ok=True)
    except (OSError, ValueError):
        return None
    if not isinstance(theme, dict):
        return None

    @lru_cache(maxsize=None)
    def asset(stem):
        if not stem or Path(stem).name != stem:
            return None
        for directory, extension in (("png", ".png"), ("icons", ".svg")):
            icon = PACK / directory / (stem + extension)
            if cache_icon(icon):
                return icon.name
        return None

    def pair(closed, opened):
        return tuple(asset(stem) if isinstance(stem, str) else None for stem in (closed, opened))

    closed = theme.get("folderNames")
    opened = theme.get("folderNamesExpanded")
    closed = closed if isinstance(closed, dict) else {}
    opened = opened if isinstance(opened, dict) else {}
    names, styles, indices = {}, [], {}
    # Thousands of names share a few hundred pairs. Publish each pair once.
    for name in closed.keys() | opened.keys():
        style = pair(closed.get(name), opened.get(name))
        if any(style):
            if style not in indices:
                styles.append(style)
                indices[style] = len(styles)
            names[name.lower()] = indices[style]
    return {
        "names": names,
        "styles": styles,
        "default": pair(theme.get("folder"), theme.get("folderExpanded")),
        "root": pair(theme.get("rootFolder"), theme.get("rootFolderExpanded")),
    }


def main() -> None:
    folders = folder_theme()
    previous = None
    while True:
        icons = sync()
        if icons != previous:
            print(json.dumps({"dir": str(CACHE), "icons": icons, "folders": folders}), flush=True)
            previous = icons
        ready, _, _ = select.select([sys.stdin], [], [], 10.0)
        if ready:
            line = sys.stdin.readline()
            if line == "":
                return


if __name__ == "__main__":
    main()
