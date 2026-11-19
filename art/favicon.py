#!/usr/bin/env python3
"""Draws kube-city's favicon (#66): three isometric towers on a tile of
grass, on a sky-blue rounded square, in the colours of the Kenney look.

Our own drawing (MIT OR Apache-2.0, like the rest of the repo). Writes
`assets/favicon/favicon.svg` and `assets/favicon/favicon.ico` (16 and 32 px
PNGs), which the binary serves; with a second argument also an 8x preview
of both pixel sizes there (`art/out/favicon-preview.png`, git-ignored).

    python3 art/favicon.py [preview-dir]

Standard library only, so no image tools are needed. Each pixel size has its
own geometry (16 px is not a shrunk 32 px): every corner sits on a whole
pixel and every slope is 2:1, so the pixel centres never fall on an edge and
the edges come out as clean isometric steps without blur.
"""

import os
import struct
import sys
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "assets", "favicon")

SKY = "#5aa9e6"
GRASS_TOP, GRASS_LEFT, GRASS_RIGHT = "#7cc45a", "#5c9a43", "#477a34"
# Towers: lit wall, shaded wall, roof, windows on the lit wall, on the shaded one.
OFFICE = ("#f1ede4", "#bdb6a8", "#8d8983", "#6f9cc9", "#4c7298")
BRICK = ("#de8a6c", "#a95d48", "#6f4135", "#f4d9a6", "#c9a777")
GLASS = ("#9fd0f0", "#5f95bd", "#3c5f7c", "#e9f6ff", "#b8d9ef")

# Per size: background corner radius, the ground tile (centre x, front y,
# half width, thickness) and the towers back to front (centre x, front y, half
# width, height, palette, window rows every N px from M px up).
SCENES = {
    32: dict(radius=6, ground=(16, 29, 12, 2), towers=[
        (16, 23, 4, 14, OFFICE, 3, 2),
        (10, 26, 4, 8, BRICK, 3, 2),
        (22, 26, 4, 5, GLASS, 2, 2),
    ]),
    16: dict(radius=3, ground=(8, 15, 6, 1), towers=[
        (8, 12, 3, 7, OFFICE, 2, 1),
        (4, 13, 2, 4, BRICK, 2, 1),
        (12, 13, 2, 3, GLASS, 0, 0),
    ]),
}


def box(cx, by, w, h):
    """An isometric box standing on (cx, by), the front corner of its
    footprint: its left wall, right wall and top as polygons."""
    front, left, right, back = (cx, by), (cx - w, by - w / 2), (cx + w, by - w / 2), (cx, by - w)
    up = lambda p, dh=h: (p[0], p[1] - dh)
    return (
        [front, left, up(left), up(front)],
        [front, right, up(right), up(front)],
        [up(front), up(left), up(back), up(right)],
    )


def band(cx, by, w, side, v, dv=1):
    """A row of windows on a wall: the strip from v to v + dv px up."""
    s = (cx - w, by - w / 2) if side < 0 else (cx + w, by - w / 2)
    inset = 1  # windows stop a pixel short of the wall's corners
    t0, t1 = inset / w, 1 - inset / w
    a = (cx + (s[0] - cx) * t0, by + (s[1] - by) * t0)
    b = (cx + (s[0] - cx) * t1, by + (s[1] - by) * t1)
    return [(a[0], a[1] - v), (b[0], b[1] - v), (b[0], b[1] - v - dv), (a[0], a[1] - v - dv)]


def shapes(size):
    """The picture as (colour, polygon) pairs, back to front."""
    s = SCENES[size]
    out = []
    gx, gy, gw, gt = s["ground"]
    left, right, top = box(gx, gy, gw, gt)
    out += [(GRASS_LEFT, left), (GRASS_RIGHT, right), (GRASS_TOP, top)]
    for cx, by, w, h, pal, every, start in s["towers"]:
        by_top = by - gt
        left, right, top = box(cx, by_top, w, h)
        out += [(pal[0], left), (pal[1], right), (pal[2], top)]
        if every:
            for v in range(start, h - 1, every):
                out.append((pal[3], band(cx, by_top, w, -1, v)))
                out.append((pal[4], band(cx, by_top, w, 1, v)))
    return out


def inside(poly, x, y):
    hit = False
    for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
        if (y0 > y) != (y1 > y) and x < x0 + (y - y0) * (x1 - x0) / (y1 - y0):
            hit = not hit
    return hit


def in_rounded(size, r, x, y):
    cx = min(max(x, r), size - r)
    cy = min(max(y, r), size - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def rgb(hex_colour):
    return tuple(int(hex_colour[i : i + 2], 16) for i in (1, 3, 5))


def raster(size):
    """RGBA rows: the background's corners anti-aliased, the rest crisp."""
    radius = SCENES[size]["radius"]
    pic = shapes(size)
    rows = []
    for y in range(size):
        row = []
        for x in range(size):
            colour = SKY
            for fill, poly in pic:
                if inside(poly, x + 0.5, y + 0.5):
                    colour = fill
            n = 4
            cover = sum(
                in_rounded(size, radius, x + (i + 0.5) / n, y + (j + 0.5) / n) for i in range(n) for j in range(n)
            )
            row.append(rgb(colour) + (round(255 * cover / (n * n)),))
        rows.append(row)
    return rows


def png(rows):
    raw = b"".join(b"\0" + bytes(c for px in row for c in px) for row in rows)
    chunk = lambda kind, data: (
        struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )
    header = struct.pack(">IIBBBBB", len(rows[0]), len(rows), 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def ico(images):
    """An .ico holding PNGs (every browser reads those)."""
    head = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, blobs = b"", b""
    for size, data in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
        blobs += data
    return head + entries + blobs


def svg():
    """The 32 px picture as a vector, for sharp tabs at any DPI."""
    r = SCENES[32]["radius"]
    parts = [f'<rect width="32" height="32" rx="{r}" fill="{SKY}" shape-rendering="auto"/>']
    for fill, poly in shapes(32):
        points = " ".join(f"{x:g},{y:g}" for x, y in poly)
        parts.append(f'<path fill="{fill}" d="M{points}Z"/>')
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" shape-rendering="crispEdges">\n'
        "<!-- kube-city favicon, drawn by art/favicon.py (MIT OR Apache-2.0) -->\n"
        + "\n".join(parts)
        + "\n</svg>\n"
    )


def preview(scale=8, gap=8):
    """Both sizes side by side, on a light and on a dark strip."""
    sizes = [16, 32]
    width = sum(s * scale for s in sizes) + gap * (len(sizes) + 1)
    height = 2 * (32 * scale + 2 * gap)
    rows = []
    for strip, bg in enumerate([(240, 240, 240), (35, 35, 40)]):
        pics = [raster(s) for s in sizes]
        for y in range(32 * scale + 2 * gap):
            x0 = gap
            px_row = [bg] * width
            for s, pic in zip(sizes, pics):
                py = (y - gap) // scale
                if 0 <= y - gap and py < s:
                    for x in range(s * scale):
                        r, g, b, a = pic[py][x // scale]
                        px_row[x0 + x] = tuple(round(c * a / 255 + k * (1 - a / 255)) for c, k in zip((r, g, b), bg))
                x0 += s * scale + gap
            rows.append([p + (255,) for p in px_row])
    assert len(rows) == height
    return png(rows)


def main():
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "favicon.ico"), "wb") as f:
        f.write(ico([(s, png(raster(s))) for s in (16, 32)]))
    with open(os.path.join(OUT, "favicon.svg"), "w") as f:
        f.write(svg())
    if len(sys.argv) > 1:
        os.makedirs(sys.argv[1], exist_ok=True)
        with open(os.path.join(sys.argv[1], "favicon-preview.png"), "wb") as f:
            f.write(preview())


if __name__ == "__main__":
    main()
