#!/usr/bin/env python3
"""Keeps the Material Icon Theme PNGs in the app's image cache.

The icons are laid out under `~/.config/sheprd/material/png/`; the mod's
mapping names them, and the provider publishes which ones are cached.
Runs until the app closes stdin, copying what is missing.
"""

import json
import os
import select
import shutil
import sys
import time
from pathlib import Path

HOME = Path(os.path.expanduser("~"))
SOURCE = HOME / ".config" / "sheprd" / "material" / "png"
CACHE = Path(os.environ.get("XDG_CACHE_HOME") or HOME / ".cache") / "sheprd-moonframe" / "images" / "material"


def sync() -> dict[str, bool]:
    icons = {}
    try:
        CACHE.mkdir(parents=True, exist_ok=True)
    except OSError:
        return icons
    for icon in SOURCE.glob("*.png"):
        dest = CACHE / icon.name
        try:
            if not dest.is_file() or dest.stat().st_size != icon.stat().st_size:
                shutil.copyfile(icon, dest)
        except OSError:
            continue
        icons[icon.name] = True
    return icons


def main() -> None:
    previous = None
    while True:
        icons = sync()
        if icons != previous:
            print(json.dumps({"dir": str(CACHE), "icons": icons}), flush=True)
            previous = icons
        ready, _, _ = select.select([sys.stdin], [], [], 10.0)
        if ready:
            line = sys.stdin.readline()
            if line == "":
                return


if __name__ == "__main__":
    main()
