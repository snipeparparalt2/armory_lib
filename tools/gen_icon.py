#!/usr/bin/env python3
"""Draws the registry icon, docs/images/icon.png (256x256): an anvil under a glowing rune,
pixel art on a dark tile. Original artwork; needs Pillow."""
from pathlib import Path
from PIL import Image

ROWS = [
    "................................",
    "................................",
    "...............cc...............",
    "..............cCCc..............",
    ".............cCWWCc.............",
    "............cCW..WCc............",
    "...........cCW.cc.WCc...........",
    "............cCW..WCc............",
    ".............cCWWCc.............",
    "..............cCCc..............",
    "...............cc...............",
    "................................",
    "................................",
    "....hhhhhhhhhhhhhhhhhhhhhhh.....",
    "...hHHHHHHHHHHHHHHHHHHHHHHHh....",
    "..hHHHHHHHHHHHHHHHHHHHHHHHHmd...",
    "...mmmmmmmmmmmmmmmmmmmmmmmmd....",
    "....ddmmmmmmmmmmmmmmmmmmddd.....",
    "......dddmmmmmmmmmmmmmmdd.......",
    ".........mmmmmmmmmmmmmm.........",
    "..........mmmmmmmmmmmm..........",
    "..........hmmmmmmmmmmd..........",
    "..........hmmmmmmmmmmd..........",
    ".........hmmmmmmmmmmmmd.........",
    "........hHmmmmmmmmmmmmmd........",
    ".......hHHmmmmmmmmmmmmmmd.......",
    "......hHHHHHHHHHHHHHHHHmmd......",
    "......mmmmmmmmmmmmmmmmmmmd......",
    "......dddddddddddddddddddd......",
    "................................",
    "................................",
    "................................",
]
COLOURS = {
    "h": (190, 196, 214), "H": (150, 157, 178), "m": (104, 110, 132), "d": (62, 66, 84),
    "c": (70, 200, 190), "C": (140, 255, 226), "W": (235, 255, 250),
}


def main():
    size = 32
    img = Image.new("RGBA", (size, size))
    for y in range(size):
        for x in range(size):
            # The tile: a dark slate with a lighter rim and a soft glow under the rune.
            edge = min(x, y, size - 1 - x, size - 1 - y)
            base = (44, 40, 66) if edge == 0 else (30, 27, 46) if edge == 1 else (22, 20, 34)
            glow = max(0.0, 1.0 - (((x - 15.5) ** 2 + (y - 6.5) ** 2) ** 0.5) / 9.0)
            base = tuple(int(c + (t - c) * glow * 0.35) for c, t in zip(base, (90, 230, 210)))
            img.putpixel((x, y), COLOURS.get(ROWS[y][x], base) + (255,))
    out = Path(__file__).resolve().parents[1] / "docs" / "images" / "icon.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    img.resize((256, 256), Image.NEAREST).save(out, optimize=True)
    print(out, out.stat().st_size, "bytes")


if __name__ == "__main__":
    assert all(len(r) == 32 for r in ROWS) and len(ROWS) == 32
    main()
