#!/usr/bin/env python3
"""Keeps the Material Icon Theme PNGs in the app's image cache.

The icons are laid out under `~/.config/sheprd/material/png/`; the mod's
mapping names them, and only the image cache is a folder a mod may show
pictures from. Runs until the app closes stdin, copying what is missing.
"""

import os
import select
import shutil
import sys
import time
from pathlib import Path

HOME = Path(os.path.expanduser("~"))
SOURCE = HOME / ".config" / "sheprd" / "material" / "png"
CACHE = Path(os.environ.get("XDG_CACHE_HOME") or HOME / ".cache") / "sheprd" / "images" / "material"


def sync() -> None:
    try:
        CACHE.mkdir(parents=True, exist_ok=True)
        for icon in SOURCE.glob("*.png"):
            dest = CACHE / icon.name
            if not dest.is_file() or dest.stat().st_size != icon.stat().st_size:
                shutil.copyfile(icon, dest)
    except Exception:
        pass


def main() -> None:
    sync()
    while True:
        ready, _, _ = select.select([sys.stdin], [], [], 10.0)
        if ready:
            line = sys.stdin.readline()
            if line == "":
                return
        sync()


if __name__ == "__main__":
    main()
