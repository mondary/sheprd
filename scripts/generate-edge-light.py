#!/usr/bin/env python3
"""Regenerate the small, transparent edge-light textures (Pillow with WebP)."""
from math import cos, exp, pi
from pathlib import Path

from PIL import Image


OUT = Path(__file__).resolve().parents[1] / "crates/sheprd-gpui/moonframe/app/mods/edge-light"
WIDTH, HEIGHT = 144, 96
PERIMETER = 2 * (WIDTH + HEIGHT)


def animation(name, size, frames, delay_ms, pixel):
    images = []
    for index in range(frames):
        phase = index / frames
        frame = Image.new("RGBA", size)
        frame.putdata([pixel(x, y, phase) for y in range(size[1]) for x in range(size[0])])
        images.append(frame)
    images[0].save(OUT / f"{name}.webp", save_all=True, append_images=images[1:],
                   duration=delay_ms, loop=0, lossless=True, background=(0, 0, 0, 0))


def comet(side, start, length, horizontal, backwards):
    # One shared four-second lap. Each side stores only its five-pixel strip,
    # not a window-sized transparent texture for every frame.
    def pixel(x, y, phase):
        along, across = (x, y) if horizontal else (y, x)
        if backwards:
            along = length - 1 - along
        if side in ("bottom", "right"):
            across = 4 - across
        distance = (phase + 0.11 - (start + along + 0.5) / PERIMETER + 0.5) % 1 - 0.5
        light = exp(-distance / 0.042) if distance >= 0 else exp(-(distance / 0.009) ** 2)
        alpha = round(255 * light * (0.88, 1, 0.75, 0.43, 0.16)[across])
        return (round(114 + 101 * light), round(114 + 130 * light), 255, alpha)

    animation(f"working-{side}", (length, 5) if horizontal else (5, length), 100, 40, pixel)


def confirmation(horizontal):
    # A green wave parts at the middle and meets at the corners, followed by rest.
    def pixel(x, y, phase):
        position = abs(2 * ((x if horizontal else y) + 0.5) / WIDTH - 1)
        travel = 1.8 * phase - 0.12
        intensity = exp(-((position - travel) / 0.19) ** 2)
        return (104, 246, 184, round(20 + 201 * intensity))

    animation("done-horizontal" if horizontal else "done-vertical",
              (WIDTH, 1) if horizontal else (1, WIDTH), 72, 55, pixel)


def blocked(x, y, phase):
    # Two soft attention beats, never a full-window flash.
    beats = exp(-((phase - 0.15) / 0.075) ** 2) + 0.72 * exp(-((phase - 0.42) / 0.08) ** 2)
    return (255, 160, 66, round(32 + 190 * beats))


def idle(x, y, phase):
    return (151, 180, 203, round(17 + 15 * (1 - cos(2 * pi * phase))))


if __name__ == "__main__":
    Image.new("RGBA", (1, 1)).save(OUT / "clear.png")
    comet("top", 0, WIDTH, True, False)
    comet("right", WIDTH, HEIGHT, False, False)
    comet("bottom", WIDTH + HEIGHT, WIDTH, True, True)
    comet("left", 2 * WIDTH + HEIGHT, HEIGHT, False, True)
    confirmation(True)
    confirmation(False)
    animation("blocked", (1, 1), 64, 50, blocked)
    animation("idle", (1, 1), 64, 100, idle)
