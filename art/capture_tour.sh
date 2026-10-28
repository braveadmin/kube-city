#!/usr/bin/env bash
# Records the #tour in Windows Chrome from WSL and writes it as an MP4.
#
#   kube-city --demo --seed 7 --no-open &
#   art/capture_tour.sh [URL] [out.mp4]
#   art/make_gif.sh target/tour.mp4 START
#
# A Chrome window with a throwaway profile opens for about 40 s; leave it
# uncovered and the mouse off it. The frames come from Chrome itself
# (art/capture_tour.ps1), drawn with the GPU; a headless Chrome in WSL draws
# WebGL on the CPU and can freeze the machine. Needs pwsh and ffmpeg.
set -euo pipefail

url=${1:-http://127.0.0.1:7700/#kenney.tour}
out=${2:-target/tour.mp4}
here=$(dirname "$(realpath "$0")")
frames=$(wslpath "$(cmd.exe /c echo %TEMP% 2>/dev/null | tr -d '\r')")/kube-city-capture

pwsh.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$here/capture_tour.ps1")" -Url "$url" | tr -d '\r'

# Each frame lasts until the next one was painted; ffmpeg then resamples to 30 fps.
list=$(mktemp)
trap 'rm -f "$list"' EXIT
python3 - "$frames" "$list" <<'EOF'
import sys
frames, out = sys.argv[1], sys.argv[2]
rows = [line.split() for line in open(f"{frames}/times.txt")]
with open(out, "w") as f:
    for (n, t), (_, t_next) in zip(rows, rows[1:] + [[None, float(rows[-1][1]) + 1 / 30]]):
        f.write(f"file '{frames}/frame-{n}.jpg'\nduration {float(t_next) - float(t):.4f}\n")
EOF
mkdir -p "$(dirname "$out")"
ffmpeg -loglevel error -y -f concat -safe 0 -i "$list" -vf "fps=30,scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p" "$out"
rm -rf "$frames"
echo "$out: $(ffprobe -v error -show_entries format=duration -of csv=p=0 "$out") s"
