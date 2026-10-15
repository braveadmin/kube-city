#!/usr/bin/env python3
"""Renders kube-city building-style samples as PNG files.

Every canvas pixel is one half of a terminal cell (the `▀` trick kube-city
uses), so the images show exactly the detail a terminal can hold. They are
upscaled only so they are comfortable to look at.

    python3 art/samples.py art/out
"""

import math
import os
import random
import struct
import sys
import zlib

TW, TH = 16, 8  # tile size in pixels: 16 terminal columns × 4 rows
W, H = 112, 96  # scene size in pixels: 112 columns × 48 rows
SCALE = 6


# --- colours --------------------------------------------------------------

def clamp(v):
    return max(0, min(255, int(round(v))))


def shade(c, f):
    return tuple(clamp(v * f) for v in c)


def mix(a, b, t):
    return tuple(clamp(x + (y - x) * t) for x, y in zip(a, b))


def hexc(s):
    s = s.lstrip("#")
    return tuple(int(s[i:i + 2], 16) for i in (0, 2, 4))


# --- canvas ---------------------------------------------------------------

class Canvas:
    def __init__(self, w, h, bg):
        self.w, self.h = w, h
        self.px = [[bg] * w for _ in range(h)]

    def set(self, x, y, c):
        x, y = int(math.floor(x)), int(math.floor(y))
        if c is not None and 0 <= x < self.w and 0 <= y < self.h:
            self.px[y][x] = c

    def blend(self, x, y, c, a):
        x, y = int(math.floor(x)), int(math.floor(y))
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[y][x] = mix(self.px[y][x], c, a)

    def poly(self, pts, colour):
        """Fills a polygon, sampling pixel centres; `colour` may be a function of (x, y)."""
        n = len(pts)
        ys = [p[1] for p in pts]
        for y in range(max(0, math.floor(min(ys))), min(self.h, math.ceil(max(ys)) + 1)):
            cy = y + 0.5
            xs = []
            for i in range(n):
                (x1, y1), (x2, y2) = pts[i], pts[(i + 1) % n]
                if (y1 <= cy < y2) or (y2 <= cy < y1):
                    xs.append(x1 + (cy - y1) * (x2 - x1) / (y2 - y1))
            xs.sort()
            for a, b in zip(xs[0::2], xs[1::2]):
                for x in range(math.ceil(a - 0.5), math.ceil(b - 0.5)):
                    self.set(x, y, colour(x, y) if callable(colour) else colour)

    def line(self, p0, p1, c, dash=None):
        x0, y0 = (int(round(v)) for v in p0)
        x1, y1 = (int(round(v)) for v in p1)
        dx, dy = abs(x1 - x0), -abs(y1 - y0)
        sx, sy = (1 if x0 < x1 else -1), (1 if y0 < y1 else -1)
        err, step = dx + dy, 0
        while True:
            if dash is None or (step // dash) % 2 == 0:
                self.set(x0, y0, c)
            if x0 == x1 and y0 == y1:
                break
            e2 = 2 * err
            if e2 >= dy:
                err += dy
                x0 += sx
            if e2 <= dx:
                err += dx
                y0 += sy
            step += 1

    def ellipse(self, cx, cy, rx, ry, colour):
        """Filled ellipse; `colour` is a function of t in -1..1 (left to right)."""
        if rx <= 0 or ry <= 0:
            return
        for y in range(math.floor(cy - ry), math.ceil(cy + ry) + 1):
            dy = (y + 0.5 - cy) / ry
            if abs(dy) >= 1:
                continue
            half = rx * math.sqrt(1 - dy * dy)
            for x in range(math.ceil(cx - half - 0.5), math.ceil(cx + half - 0.5)):
                self.set(x, y, colour((x + 0.5 - cx) / rx))

    def glow(self, x, y, c, radius, strength):
        for gy in range(int(y - radius), int(y + radius) + 1):
            for gx in range(int(x - radius * 2), int(x + radius * 2) + 1):
                d = math.hypot((gx - x) / 2, gy - y) / radius
                if d < 1:
                    self.blend(gx, gy, c, strength * (1 - d) ** 2)

    def save(self, path, scale):
        rows = []
        for row in self.px:
            line = b"".join(bytes(c) * scale for c in row)
            rows.extend([b"\x00" + line] * scale)

        def chunk(tag, data):
            return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

        header = struct.pack(">IIBBBBB", self.w * scale, self.h * scale, 8, 2, 0, 0, 0)
        with open(path, "wb") as f:
            f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
                    + chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b""))


# --- isometric scene ------------------------------------------------------

class Scene:
    """World units: one tile = 1 in x and y; z is in pixels."""

    def __init__(self, bg):
        self.cv = Canvas(W, H, bg)
        self.ox, self.oy = W // 2, 46

    def P(self, x, y, z=0):
        return (self.ox + (x - y) * TW / 2, self.oy + (x + y) * TH / 2 - z)

    def tile(self, x0, y0, x1, y1, colour, z=0):
        self.cv.poly([self.P(x0, y0, z), self.P(x1, y0, z), self.P(x1, y1, z), self.P(x0, y1, z)], colour)

    def face_left(self, x0, x1, y, z0, z1, fn):
        """The face on plane y between x0 and x1 (seen on the left)."""
        ax, ay = self.P(x0, y)
        cols = int(round((x1 - x0) * TW / 2))
        for i in range(cols):
            bottom = ay + (i + 0.5) / 2
            for z in range(int(z0), int(z1)):
                self.cv.set(ax + i, math.floor(bottom - z), fn(i, z - z0, cols, z1 - z0))

    def face_right(self, x, y0, y1, z0, z1, fn):
        """The face on plane x between y1 (front) and y0 (back), seen on the right."""
        ax, ay = self.P(x, y1)
        cols = int(round((y1 - y0) * TW / 2))
        for i in range(cols):
            bottom = ay - (i + 0.5) / 2
            for z in range(int(z0), int(z1)):
                self.cv.set(ax + i, math.floor(bottom - z), fn(i, z - z0, cols, z1 - z0))

    def box(self, x0, y0, x1, y1, z0, z1, top, left, right):
        flat = lambda c: c if callable(c) else (lambda i, z, n, h: c)
        self.face_left(x0, x1, y1, z0, z1, flat(left))
        self.face_right(x1, y0, y1, z0, z1, flat(right))
        self.tile(x0, y0, x1, y1, top, z1)

    def edges(self, x0, y0, x1, y1, z0, z1, colour, verticals=True):
        P, ln = self.P, self.cv.line
        for a, b in [((x0, y0), (x1, y0)), ((x1, y0), (x1, y1)), ((x1, y1), (x0, y1)), ((x0, y1), (x0, y0))]:
            ln(P(*a, z1), P(*b, z1), colour)
        if verticals:
            for x, y in [(x0, y1), (x1, y1), (x1, y0)]:
                ln(P(x, y, z0), P(x, y, z1 - 1), colour)

    def gable(self, x0, y0, x1, y1, z, rise, roof, roof_dark, wall):
        """A pitched roof with its ridge along x."""
        P, ym = self.P, (y0 + y1) / 2
        self.cv.poly([P(x0, y0, z), P(x1, y0, z), P(x1, ym, z + rise), P(x0, ym, z + rise)], roof_dark)
        self.cv.poly([P(x1, y0, z), P(x1, y1, z), P(x1, ym, z + rise)], wall)
        self.cv.poly([P(x0, y1, z), P(x1, y1, z), P(x1, ym, z + rise), P(x0, ym, z + rise)], roof)

    def revolve(self, cx, cy, z0, z1, radius, side, top=None):
        """A round solid: stacked ellipses, radius(z) in tiles, side(t, z) colours."""
        sx, sy = self.P(cx, cy)
        for z in range(int(z0), int(z1)):
            r = radius(z)
            self.cv.ellipse(sx, sy - z, r * math.sqrt(2) * TW / 2, r * math.sqrt(2) * TH / 2,
                            lambda t, z=z: side(t, z))
        if top is not None:
            r = radius(z1)
            self.cv.ellipse(sx, sy - z1, r * math.sqrt(2) * TW / 2, r * math.sqrt(2) * TH / 2, lambda t: top)


def window_grid(base, window, floor=4, win_h=2, period=3, width=2, bottom=3, top=3, edge=1):
    """A face pattern: `window(col, row)` returns a colour (or None for wall)."""
    def fn(i, z, n, h):
        if z < bottom or z >= h - top or (z - bottom) % floor >= win_h:
            return base
        if i < edge or i >= n - edge or (i - edge) % period >= width:
            return base
        return window((i - edge) // period, (z - bottom) // floor) or base
    return fn


# --- the sample block -----------------------------------------------------

# One namespace block (4×4 lots) with roads around it. Same scene per style.
LOTS = [
    ((0, 0), "tower", dict(workload="api-gateway", height=40, tiers=3, state="running")),
    ((1, 0), "tower", dict(workload="catalog-api", height=26, state="running")),
    ((0, 1), "tower", dict(workload="catalog-api", height=26, state="running")),
    ((2, 0), "storage", dict(workload="postgres", height=18, state="running")),
    ((0, 2), "storage", dict(workload="postgres", height=18, state="running")),
    ((3, 0), "tower", dict(workload="search-indexer", height=30, tiers=2, state="crashloop")),
    ((0, 3), "scaffold", dict(height=14)),
    ((1, 1), "park", {}),
    ((2, 1), "job", dict(state="running")),
    ((1, 2), "tower", dict(workload="old-console", height=18, state="terminating")),
    ((3, 1), "house", dict(workload="node-exporter", state="running")),
    ((1, 3), "house", dict(workload="node-exporter", state="running")),
    ((2, 2), "done", dict(state="succeeded")),
    ((3, 2), "park", {}),
    ((2, 3), "park", {}),
    ((3, 3), "house", dict(workload="node-exporter", state="running")),
]


def wl_hash(name):
    h = 2166136261
    for b in name.encode():
        h = ((h ^ b) * 16777619) & 0xFFFFFFFF
    return h


class Style:
    name = ""
    bg = (0, 0, 0)

    def __init__(self):
        self.rng = random.Random(7)

    def render(self, path):
        sc = Scene(self.bg)
        self.ground(sc)
        for (x, y), kind, opts in sorted(LOTS, key=lambda lot: (lot[0][0] + lot[0][1], lot[0][0])):
            getattr(self, kind)(sc, x, y, **opts)
        self.finish(sc)
        sc.cv.save(path, SCALE)

    def ground(self, sc):
        c = self.colours
        for ty in range(-1, 5):
            for tx in range(-1, 5):
                if tx in (-1, 4) or ty in (-1, 4):
                    sc.tile(tx, ty, tx + 1, ty + 1, c["road"])
        # Centre lines along both roads in front and behind the block.
        for t in range(-1, 5):
            for fixed in (-1, 4):
                sc.cv.line(sc.P(t + 0.15, fixed + 0.5), sc.P(t + 0.55, fixed + 0.5), c["line"])
                sc.cv.line(sc.P(fixed + 0.5, t + 0.15), sc.P(fixed + 0.5, t + 0.55), c["line"])
        sc.tile(0, 0, 4, 4, c["sidewalk"])
        for ty in range(4):
            for tx in range(4):
                sc.tile(tx + 0.125, ty + 0.125, tx + 0.875, ty + 0.875, c["lot"])

    def finish(self, sc):
        pass


# --- style 1: pixel metropolis (daylight, SimCity 2000-ish) ----------------

class Metropolis(Style):
    name = "metropolis"
    bg = hexc("1e2433")
    colours = dict(road=hexc("4a4d55"), line=hexc("e6c84a"), sidewalk=hexc("b9b4a8"), lot=hexc("6f9a4f"))
    facades = [hexc("d8c7a3"), hexc("7fa3c8"), hexc("b2604a"), hexc("e4e2dc"), hexc("6fb0ad")]
    glass, lit = hexc("3d5878"), hexc("f6e39a")

    def tower(self, sc, x, y, workload, height, state, tiers=1):
        h = wl_hash(workload)
        facade = self.facades[h % len(self.facades)]
        if state == "terminating":
            facade = mix(facade, hexc("8a8a8a"), 0.6)
            height = int(height * 0.7)
        dark = state != "running"
        lit_rate = 0.0 if dark else 0.25

        def window(c, r, f=facade):
            if state == "crashloop" and self.rng.random() < 0.3:
                return hexc("ff8a3a")
            return self.lit if self.rng.random() < lit_rate else (shade(self.glass, 0.6) if dark else self.glass)

        insets = [0.18, 0.28, 0.38][:tiers]
        z0 = 0
        for k, inset in enumerate(insets):
            z1 = height if k == tiers - 1 else int(height * (0.55 + 0.2 * k))
            x0, y0, x1, y1 = x + inset, y + inset, x + 1 - inset, y + 1 - inset
            left = window_grid(shade(facade, 0.72), window)
            right = window_grid(shade(facade, 0.9), window)
            if k == 0:
                # Ground floor: darker band with shop windows.
                base_l, base_r = left, right
                left = lambda i, z, n, hh, f=base_l: (hexc("2c3440") if z < 3 and i % 3 else f(i, z, n, hh))
                right = lambda i, z, n, hh, f=base_r: (hexc("2c3440") if z < 3 and i % 3 else f(i, z, n, hh))
            sc.box(x0, y0, x1, y1, z0, z1, shade(facade, 1.08), left, right)
            # Parapet rim and roof gravel.
            sc.edges(x0, y0, x1, y1, z0, z1, shade(facade, 1.25), verticals=False)
            sc.tile(x0 + 0.06, y0 + 0.06, x1 - 0.06, y1 - 0.06, shade(facade, 0.95), z1)
            sc.cv.line(sc.P(x1, y1, z0), sc.P(x1, y1, z1 - 1), shade(facade, 1.15))
            z0 = z1
        top = insets[-1]
        if state == "terminating":
            # Half demolished: jagged top and a dust cloud.
            for i in range(14):
                sc.cv.set(*sc.P(x + top + self.rng.random() * (1 - 2 * top), y + 0.5, height + self.rng.random() * 3),
                          hexc("c8b89a"))
            return
        if tiers >= 2:
            sc.box(x + 0.5, y + 0.45, x + 0.62, y + 0.57, height, height + 4, hexc("9aa3ad"), hexc("6d7580"), hexc("858d97"))
        if tiers == 3:
            ax, ay = sc.P(x + 0.5, y + 0.5, height)
            sc.cv.line((ax, ay), (ax, ay - 9), hexc("c0c6cc"))
            sc.cv.set(ax, ay - 10, hexc("ff4040"))
            sc.revolve(x + 0.32, y + 0.32, height, height + 5, lambda z: 0.09,
                       lambda t, z: shade(hexc("8a6a4a"), 0.8 + 0.3 * t), hexc("a5825c"))
        if state == "crashloop":
            self.fire(sc, x + 0.5, y + 0.5, height)

    def fire(self, sc, x, y, z):
        cx, cy = sc.P(x, y, z)
        for i in range(40):
            dx = self.rng.gauss(0, 2.2)
            dy = -abs(self.rng.gauss(0, 3.5))
            c = [hexc("ffe066"), hexc("ff9a2e"), hexc("e8452c")][min(2, int(abs(dy) / 2.2))]
            sc.cv.set(cx + dx, cy + dy, c)
        for i in range(26):
            sc.cv.set(cx + 2 + self.rng.gauss(i * 0.15, 1.5), cy - 7 - i * 0.6 + self.rng.gauss(0, 1),
                      mix(hexc("4a4a50"), hexc("8a8a90"), self.rng.random()))

    def storage(self, sc, x, y, workload, height, state):
        body = hexc("b8bec6")
        side = lambda t, z: shade(hexc("8e959e") if z % 6 == 5 else body, 0.7 + 0.35 * (t + 1) / 2)
        sc.revolve(x + 0.5, y + 0.5, 0, height, lambda z: 0.36, side, None)
        sc.revolve(x + 0.5, y + 0.5, height, height + 4, lambda z: 0.36 * math.sqrt(max(0, 1 - ((z - height) / 4) ** 2)),
                   lambda t, z: shade(hexc("d6dbe1"), 0.85 + 0.25 * (t + 1) / 2), None)
        lx, ly = sc.P(x + 0.5, y + 0.86)
        sc.cv.line((lx + 4, ly - 1), (lx + 4, ly - height + 2), hexc("5c636c"))

    def house(self, sc, x, y, workload, state):
        wall, roof = hexc("e8dcc0"), hexc("b5513e")
        door, win = hexc("6b4a32"), hexc("5d84a8")
        x0, y0, x1, y1 = x + 0.2, y + 0.25, x + 0.8, y + 0.75
        left = lambda i, z, n, h: door if (n // 2 - 1 <= i <= n // 2 and z < 4) else (win if (i in (1, n - 2) and 3 <= z <= 4) else shade(wall, 0.78))
        right = lambda i, z, n, h: win if (i in (1, 2) and 3 <= z <= 4) else shade(wall, 0.92)
        sc.box(x0, y0, x1, y1, 0, 7, wall, left, right)
        sc.box(x0 + 0.1, y0 + 0.08, x0 + 0.2, y0 + 0.18, 7, 12, hexc("8a4a3a"), hexc("7a3a2a"), hexc("8a4a3a"))
        sc.gable(x0 - 0.04, y0 - 0.04, x1 + 0.04, y1 + 0.04, 7, 6, roof, shade(roof, 0.75), shade(wall, 0.95))

    def job(self, sc, x, y, state):
        wall = hexc("a89a86")
        sc.box(x + 0.12, y + 0.18, x + 0.88, y + 0.82, 0, 10, wall,
               window_grid(shade(wall, 0.72), lambda c, r: hexc("f2c46a"), floor=5, period=4), shade(wall, 0.9))
        for k in range(3):  # sawtooth roof
            x0 = x + 0.12 + k * 0.25
            sc.cv.poly([sc.P(x0, y + 0.82, 10), sc.P(x0 + 0.25, y + 0.82, 10), sc.P(x0 + 0.25, y + 0.82, 14)], hexc("6f7a86"))
            sc.cv.poly([sc.P(x0, y + 0.18, 10), sc.P(x0, y + 0.82, 10), sc.P(x0 + 0.25, y + 0.82, 14), sc.P(x0 + 0.25, y + 0.18, 14)], hexc("8c97a3"))
        sc.revolve(x + 0.78, y + 0.28, 0, 22, lambda z: 0.08, lambda t, z: shade(hexc("9c5a48") if z % 5 else hexc("e8e8e8"), 0.75 + 0.3 * t), hexc("3a3a3a"))
        cx, cy = sc.P(x + 0.78, y + 0.28, 22)
        for i in range(30):
            sc.cv.set(cx + i * 0.25 + self.rng.gauss(0, 1), cy - 2 - i * 0.45 + self.rng.gauss(0, 0.8),
                      mix(hexc("e8e8ec"), hexc("b8b8c0"), self.rng.random()))

    def scaffold(self, sc, x, y, height):
        sc.box(x + 0.15, y + 0.15, x + 0.85, y + 0.85, 0, 4, hexc("a8a8a0"), hexc("80807a"), hexc("94948c"))
        x0, y0, x1, y1 = x + 0.12, y + 0.12, x + 0.88, y + 0.88
        orange = hexc("f0a030")
        for z in range(4, height + 1, 4):
            sc.edges(x0, y0, x1, y1, z - 4, z, orange)
        for px, py in [(x0, y1), (x1, y1), (x1, y0)]:
            sc.cv.line(sc.P(px, py, 0), sc.P(px, py, height), shade(orange, 0.8))
        # Tower crane.
        mx, my = sc.P(x + 0.3, y + 0.3, 0)
        for z in range(0, height + 16):
            sc.cv.set(mx, my - z, hexc("f2d23a") if z % 2 else hexc("b89a20"))
        sc.cv.line((mx - 6, my - height - 15), (mx + 14, my - height - 15), hexc("f2d23a"))
        sc.cv.line((mx + 11, my - height - 15), (mx + 11, my - height - 6), hexc("6a6a6a"))
        sc.cv.set(mx - 6, my - height - 14, hexc("6a6a6a"))

    def park(self, sc, x, y):
        for i, (dx, dy) in enumerate([(0.3, 0.35), (0.7, 0.4), (0.45, 0.72)]):
            self.tree(sc, x + dx, y + dy, 1.0 - 0.15 * i)

    def tree(self, sc, x, y, size):
        tx, ty = sc.P(x, y)
        sc.cv.line((tx, ty), (tx, ty - 3), hexc("6b4a2e"))
        r = 3.2 * size
        sc.cv.ellipse(tx, ty - 5, r * 1.3, r, lambda t: hexc("3f7a35") if t < -0.2 else hexc("58a046") if t < 0.5 else hexc("7cc25a"))

    def done(self, sc, x, y, state):
        sc.box(x + 0.2, y + 0.2, x + 0.8, y + 0.8, 0, 2, hexc("c8c4b8"), hexc("98948a"), hexc("aca89e"))
        sc.box(x + 0.35, y + 0.35, x + 0.65, y + 0.65, 2, 6, hexc("7aa6c8"), hexc("4a7698"), hexc("5e8aac"))
        cx, cy = sc.P(x + 0.5, y + 0.5, 6)
        for dx, dy in [(-2, 0), (-1, 1), (0, 0), (1, -1), (2, -2)]:
            sc.cv.set(cx + dx, cy - 3 + dy, hexc("6fe08a"))


# --- style 2: neon night ----------------------------------------------------

class Neon(Style):
    name = "neon"
    bg = hexc("07070f")
    colours = dict(road=hexc("0c0c16"), line=hexc("00c8ff"), sidewalk=hexc("1b1c2e"), lot=hexc("12131f"))
    neons = [hexc("00e5ff"), hexc("ff3fd2"), hexc("9dff3a"), hexc("ffb020"), hexc("8a6bff")]
    win_colours = [hexc("ffcf80"), hexc("7fd6ff"), hexc("ff8ad8")]

    def __init__(self):
        super().__init__()
        self.glows = []

    def tower(self, sc, x, y, workload, height, state, tiers=1):
        neon = self.neons[wl_hash(workload) % len(self.neons)]
        if state == "crashloop":
            neon = hexc("ff2a2a")
        if state == "terminating":
            neon = hexc("3a3a4a")
        body = hexc("1a1b2c")
        lit = 0.0 if state == "terminating" else 0.45

        def window(c, r):
            if state == "crashloop":
                return hexc("ff3a3a") if self.rng.random() < 0.4 else hexc("2a1015")
            if self.rng.random() < lit:
                return self.rng.choice(self.win_colours)
            return hexc("10111c")

        insets = [0.18, 0.28, 0.38][:tiers]
        z0 = 0
        for k, inset in enumerate(insets):
            z1 = height if k == tiers - 1 else int(height * (0.55 + 0.2 * k))
            x0, y0, x1, y1 = x + inset, y + inset, x + 1 - inset, y + 1 - inset
            sc.box(x0, y0, x1, y1, z0, z1, hexc("141522"),
                   window_grid(shade(body, 0.8), window, period=2, width=1),
                   window_grid(body, window, period=2, width=1))
            sc.edges(x0, y0, x1, y1, z0, z1, neon, verticals=False)
            sc.cv.line(sc.P(x1, y1, z0), sc.P(x1, y1, z1 - 1), shade(neon, 0.8))
            for px, py in [(x0, y1), (x1, y1), (x1, y0)]:
                self.glows.append((*sc.P(px, py, z1), neon, 2.2, 0.35))
            z0 = z1
        top = insets[-1]
        if state == "terminating":
            return
        # A sign on the roof, facing the viewer.
        sx, sy = sc.P(x + top + 0.1, y + 1 - top, height)
        sc.cv.poly([(sx, sy - 1), (sx + 5, sy + 1.5), (sx + 5, sy - 2.5), (sx, sy - 5)], neon)
        self.glows.append((sx + 2.5, sy - 2, neon, 4, 0.4))
        ax, ay = sc.P(x + 0.5, y + 0.5, height)
        sc.cv.line((ax + 2, ay), (ax + 2, ay - 6), hexc("44465a"))
        sc.cv.set(ax + 2, ay - 7, hexc("ff3030"))
        self.glows.append((ax + 2, ay - 7, hexc("ff3030"), 2.5, 0.6))
        if state == "crashloop":
            cx, cy = sc.P(x + 0.5, y + 0.5, height)
            self.glows.append((cx, cy - 4, hexc("ff2a2a"), 9, 0.55))
            for i in range(24):
                sc.cv.set(cx + self.rng.gauss(0, 2), cy - abs(self.rng.gauss(0, 3)) - 1,
                          self.rng.choice([hexc("ff4040"), hexc("ffb040"), hexc("ffe080")]))

    def storage(self, sc, x, y, workload, height, state):
        ring = self.neons[wl_hash(workload) % len(self.neons)]
        side = lambda t, z: ring if z % 6 == 3 else shade(hexc("1c1d30"), 0.8 + 0.3 * (t + 1) / 2)
        sc.revolve(x + 0.5, y + 0.5, 0, height, lambda z: 0.34, side, hexc("1a1b2c"))
        cx, cy = sc.P(x + 0.5, y + 0.5, height)
        sc.cv.ellipse(cx, cy, 6, 3, lambda t: ring if abs(t) > 0.75 else hexc("1a1b2c"))
        self.glows.append((cx, cy - 1, ring, 5, 0.3))

    def house(self, sc, x, y, workload, state):
        body = hexc("181928")
        neon = hexc("ff3fd2")
        left = lambda i, z, n, h: neon if z == 5 and 1 <= i < n - 1 else (hexc("ffcf80") if z in (2, 3) and i % 3 == 1 else shade(body, 0.8))
        sc.box(x + 0.2, y + 0.25, x + 0.8, y + 0.75, 0, 8, hexc("121320"), left, body)
        sc.edges(x + 0.2, y + 0.25, x + 0.8, y + 0.75, 0, 8, hexc("9dff3a"), verticals=False)
        lx, ly = sc.P(x + 0.5, y + 0.75, 5)
        self.glows.append((lx, ly, neon, 5, 0.35))

    def job(self, sc, x, y, state):
        body = hexc("181928")
        vent = lambda i, z, n, h: hexc("00e5ff") if z in (3, 7) and 1 <= i < n - 1 else shade(body, 0.8)
        sc.box(x + 0.12, y + 0.18, x + 0.88, y + 0.82, 0, 11, hexc("121320"), vent, body)
        sc.revolve(x + 0.75, y + 0.3, 11, 22, lambda z: 0.08, lambda t, z: hexc("00e5ff") if z % 4 == 0 else hexc("222438"), hexc("0a0a12"))
        cx, cy = sc.P(x + 0.75, y + 0.3, 22)
        for i in range(24):
            sc.cv.blend(cx + i * 0.3 + self.rng.gauss(0, 1), cy - 2 - i * 0.5, hexc("8a6bff"), 0.5)
        self.glows.append((cx, cy, hexc("00e5ff"), 4, 0.3))

    def scaffold(self, sc, x, y, height):
        holo = hexc("00e5ff")
        x0, y0, x1, y1 = x + 0.15, y + 0.15, x + 0.85, y + 0.85
        sc.box(x0, y0, x1, y1, 0, 3, hexc("15283a"), hexc("0f1e2c"), hexc("132536"))
        P = sc.P
        for a, b in [((x0, y0), (x1, y0)), ((x1, y0), (x1, y1)), ((x1, y1), (x0, y1)), ((x0, y1), (x0, y0))]:
            sc.cv.line(P(*a, height), P(*b, height), holo, dash=1)
        for px, py in [(x0, y1), (x1, y1), (x1, y0)]:
            sc.cv.line(P(px, py, 3), P(px, py, height), shade(holo, 0.7), dash=1)
        for z in range(5, height, 4):
            sc.cv.line(P(x0, y1, z), P(x1, y1, z), shade(holo, 0.35))
        cx, cy = P(x + 0.5, y + 0.5, height / 2)
        self.glows.append((cx, cy, holo, 8, 0.2))

    def park(self, sc, x, y):
        for dx, dy in [(0.3, 0.35), (0.7, 0.45)]:
            tx, ty = sc.P(x + dx, y + dy)
            sc.cv.ellipse(tx, ty - 4, 4, 3, lambda t: hexc("0f2a22") if t < 0.3 else hexc("1a4a38"))
        lx, ly = sc.P(x + 0.5, y + 0.8)
        sc.cv.line((lx, ly), (lx, ly - 7), hexc("3a3c50"))
        sc.cv.set(lx, ly - 8, hexc("ffe9b0"))
        self.glows.append((lx, ly - 8, hexc("ffd890"), 3, 0.5))
        self.glows.append((lx, ly, hexc("ffd890"), 6, 0.18))

    def done(self, sc, x, y, state):
        sc.tile(x + 0.2, y + 0.2, x + 0.8, y + 0.8, hexc("10261a"))
        cx, cy = sc.P(x + 0.5, y + 0.5)
        sc.cv.ellipse(cx, cy, 7, 3.5, lambda t: hexc("3aff7a") if abs(t) > 0.8 else None)
        self.glows.append((cx, cy, hexc("3aff7a"), 6, 0.3))

    def finish(self, sc):
        for x, y, c, r, s in self.glows:
            sc.cv.glow(x, y, c, r, s)
        # Road centre lines glow too.
        for t in range(-1, 5):
            for fixed in (-1, 4):
                sc.cv.glow(*sc.P(t + 0.35, fixed + 0.5), hexc("00c8ff"), 2, 0.25)
                sc.cv.glow(*sc.P(fixed + 0.5, t + 0.35), hexc("00c8ff"), 2, 0.25)


# --- style 3: pastel toy town -----------------------------------------------

class Toytown(Style):
    name = "toytown"
    bg = hexc("2a3a44")
    colours = dict(road=hexc("b9b2a8"), line=hexc("f8f4ea"), sidewalk=hexc("efe6d2"), lot=hexc("a8d4a0"))
    walls = [hexc("a9dcc8"), hexc("f7bfa0"), hexc("cbbbe8"), hexc("f6dc8e"), hexc("a4c8ea")]
    roofs = [hexc("d8705a"), hexc("5aa6a8"), hexc("8a6fb8")]

    def walls_for(self, workload):
        h = wl_hash(workload)
        return self.walls[h % len(self.walls)], self.roofs[(h >> 8) % len(self.roofs)]

    def facade(self, wall, window, factor):
        base = shade(wall, factor)
        trim = mix(base, (255, 255, 255), 0.6)

        def fn(i, z, n, h):
            if z % 6 == 5:
                return trim  # white cornice between floors
            if 1 <= z % 6 <= 3 and i % 4 in (1, 2) and 0 < i < n - 1:
                return window(i // 4, z // 6)
            return base
        return fn

    def tower(self, sc, x, y, workload, height, state, tiers=1):
        wall, roof = self.walls_for(workload)
        if state == "terminating":
            wall, roof = mix(wall, hexc("c8c4c0"), 0.7), mix(roof, hexc("c8c4c0"), 0.7)
        glass = hexc("4f7d94")

        def window(c, r):
            if state == "crashloop":
                return hexc("ff9a4a") if self.rng.random() < 0.5 else hexc("5a4a4a")
            if state == "terminating":
                return shade(glass, 0.6)
            return hexc("fff2b0") if self.rng.random() < 0.2 else glass

        insets = [0.18, 0.28, 0.38][:tiers]
        z0 = 0
        for k, inset in enumerate(insets):
            z1 = height if k == tiers - 1 else int(height * (0.55 + 0.2 * k))
            x0, y0, x1, y1 = x + inset, y + inset, x + 1 - inset, y + 1 - inset
            sc.box(x0, y0, x1, y1, z0, z1, wall, self.facade(wall, window, 0.86), self.facade(wall, window, 0.97))
            z0 = z1
        top = insets[-1]
        x0, y0, x1, y1 = x + top, y + top, x + 1 - top, y + 1 - top
        if tiers >= 3:
            sc.revolve(x + 0.5, y + 0.5, height, height + 6,
                       lambda z: 0.16 * math.sqrt(max(0, 1 - ((z - height) / 6) ** 2)),
                       lambda t, z: shade(roof, 0.85 + 0.25 * (t + 1) / 2), None)
            fx, fy = sc.P(x + 0.5, y + 0.5, height + 6)
            sc.cv.line((fx, fy), (fx, fy - 4), hexc("f8f4ea"))
            sc.cv.set(fx + 1, fy - 4, hexc("e85a5a"))
            sc.cv.set(fx + 2, fy - 4, hexc("e85a5a"))
        else:
            sc.gable(x0 - 0.04, y0 - 0.04, x1 + 0.04, y1 + 0.04, height, 7, roof, shade(roof, 0.75), shade(wall, 0.97))
        if state == "crashloop":
            self.puff_fire(sc, x + 0.5, y + 0.5, height + 5)

    def puff_fire(self, sc, x, y, z):
        cx, cy = sc.P(x, y, z)
        for r, dx, dy, c in [(3, 0, 0, hexc("ff8a3a")), (2.2, -2, -2, hexc("ffb347")), (1.6, 1, -4, hexc("ffe27a"))]:
            sc.cv.ellipse(cx + dx, cy + dy, r * 1.4, r, lambda t, c=c: c)
        for r, dx, dy in [(3, 4, -8), (3.5, 7, -13), (4, 11, -19)]:
            sc.cv.ellipse(cx + dx, cy + dy, r * 1.4, r, lambda t: hexc("c9c4c0") if t < 0.3 else hexc("e8e4e0"))

    def storage(self, sc, x, y, workload, height, state):
        wall, roof = hexc("f3ead8"), hexc("d8705a")
        side = lambda t, z: shade(hexc("e88a7a") if z % 8 in (6, 7) else wall, 0.86 + 0.14 * (t + 1) / 2)
        sc.revolve(x + 0.5, y + 0.5, 0, height, lambda z: 0.3, side, None)
        sc.revolve(x + 0.5, y + 0.5, height, height + 10, lambda z: 0.36 * (1 - (z - height) / 10),
                   lambda t, z: shade(roof, 0.8 + 0.3 * (t + 1) / 2), None)
        wx, wy = sc.P(x + 0.5, y + 0.85, height - 6)
        sc.cv.set(wx - 1, wy, hexc("4f7d94"))
        sc.cv.set(wx - 1, wy - 1, hexc("4f7d94"))

    def house(self, sc, x, y, workload, state):
        wall, roof = self.walls_for(workload + "-house")
        door = hexc("8a5a3a")
        left = lambda i, z, n, h: door if (n // 2 - 1 <= i <= n // 2 and z < 4) else (hexc("4f7d94") if i in (1, n - 2) and 3 <= z <= 4 else (hexc("f07a9a") if i in (1, n - 2) and z == 2 else shade(wall, 0.86)))
        right = lambda i, z, n, h: hexc("4f7d94") if i in (1, 2) and 3 <= z <= 4 else shade(wall, 0.97)
        x0, y0, x1, y1 = x + 0.2, y + 0.25, x + 0.8, y + 0.75
        sc.box(x0, y0, x1, y1, 0, 7, wall, left, right)
        sc.gable(x0 - 0.05, y0 - 0.05, x1 + 0.05, y1 + 0.05, 7, 7, roof, shade(roof, 0.75), shade(wall, 0.97))

    def job(self, sc, x, y, state):
        # A windmill: the job's work turns the sails.
        wall = hexc("f3ead8")
        sc.revolve(x + 0.5, y + 0.5, 0, 20, lambda z: 0.32 - z * 0.008, lambda t, z: shade(wall, 0.86 + 0.14 * (t + 1) / 2), None)
        sc.revolve(x + 0.5, y + 0.5, 20, 26, lambda z: 0.17 * (1 - (z - 20) / 6), lambda t, z: shade(hexc("5aa6a8"), 0.8 + 0.3 * (t + 1) / 2), None)
        hx, hy = sc.P(x + 0.5, y + 0.82, 19)
        sail = hexc("fffaf0")
        for ang in (0.5, 2.07, 3.64, 5.21):
            ex, ey = hx + math.cos(ang) * 11, hy + math.sin(ang) * 8
            sc.cv.line((hx, hy), (ex, ey), hexc("8a5a3a"))
            sc.cv.line((hx + math.cos(ang + 0.25) * 4, hy + math.sin(ang + 0.25) * 3), (ex + math.cos(ang + 1.3) * 2, ey + math.sin(ang + 1.3) * 2), sail)
        sc.cv.set(hx, hy, hexc("8a5a3a"))

    def scaffold(self, sc, x, y, height):
        wood, plank = hexc("b98a5a"), hexc("8a6038")
        sc.box(x + 0.18, y + 0.2, x + 0.82, y + 0.8, 0, 6, hexc("f7bfa0"), shade(hexc("f7bfa0"), 0.86), shade(hexc("f7bfa0"), 0.97))
        x0, y0, x1, y1 = x + 0.12, y + 0.14, x + 0.88, y + 0.86
        for px, py in [(x0, y1), (x1, y1), (x1, y0), (x0, y0)]:
            sc.cv.line(sc.P(px, py, 0), sc.P(px, py, height), wood)
        for z in range(6, height + 1, 6):
            sc.cv.line(sc.P(x0, y1, z), sc.P(x1, y1, z), plank)
            sc.cv.line(sc.P(x1, y1, z), sc.P(x1, y0, z), plank)
        bx, by = sc.P(x + 0.5, y + 0.5, height + 2)
        sc.cv.line((bx - 8, by), (bx + 8, by), hexc("f6dc8e"))
        sc.cv.line((bx + 6, by), (bx + 6, by + 6), hexc("8a8a8a"))

    def park(self, sc, x, y):
        for i, (dx, dy) in enumerate([(0.3, 0.35), (0.72, 0.42), (0.45, 0.75)]):
            tx, ty = sc.P(x + dx, y + dy)
            sc.cv.line((tx, ty), (tx, ty - 4), hexc("a07850"))
            green = [hexc("6cbf84"), hexc("8fd18a"), hexc("5aae9a")][i]
            sc.cv.ellipse(tx, ty - 7, 4, 4, lambda t, g=green: shade(g, 0.85 + 0.25 * (t + 1) / 2))
        for k in range(8):
            fx, fy = sc.P(x + 0.15 + self.rng.random() * 0.7, y + 0.15 + self.rng.random() * 0.7)
            sc.cv.set(fx, fy, self.rng.choice([hexc("f07a9a"), hexc("ffffff"), hexc("f6dc8e")]))

    def done(self, sc, x, y, state):
        # A finished job leaves a little gazebo.
        sc.tile(x + 0.2, y + 0.2, x + 0.8, y + 0.8, hexc("efe6d2"))
        for px, py in [(0.3, 0.7), (0.7, 0.7), (0.7, 0.3)]:
            sc.cv.line(sc.P(x + px, y + py, 0), sc.P(x + px, y + py, 6), hexc("f8f4ea"))
        sc.revolve(x + 0.5, y + 0.5, 6, 11, lambda z: 0.3 * (1 - (z - 6) / 5), lambda t, z: shade(hexc("a4c8ea"), 0.82 + 0.25 * (t + 1) / 2), None)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "."
    os.makedirs(out, exist_ok=True)
    for style in (Metropolis(), Neon(), Toytown()):
        path = os.path.join(out, f"{style.name}.png")
        style.render(path)
        print(path)
