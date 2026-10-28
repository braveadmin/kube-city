#!/usr/bin/env bash
# Turns a screen recording of the #tour into the looping README GIF.
#
# 1. kube-city --demo --seed 7 --no-open, then open
#    http://127.0.0.1:7700/#kenney.tour in Chrome (normal window, about
#    1280x800, mouse off the page).
# 2. Record the browser for at least 35 s (two tours), for example with the
#    Windows Snipping Tool (Win+Shift+R, saves an MP4).
# 3. art/make_gif.sh recording.mp4 START
#    START is the second at which a tour begins (the view sits on the whole
#    city, just before it flies into a district); one tour is 15 s.
#
# Needs ffmpeg. Writes docs/demo.gif unless a third argument names another
# file. A two-pass palette keeps the colours; if the GIF comes out over 5 MB
# it is made again smaller and at fewer frames per second.
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: $0 recording.mp4 START [out.gif]" >&2
  exit 2
fi
input=$1 start=$2 out=${3:-docs/demo.gif}
duration=15
limit=$((5 * 1024 * 1024))
command -v ffmpeg >/dev/null || { echo "ffmpeg is not installed (sudo apt install ffmpeg)" >&2; exit 1; }

palette=$(mktemp --suffix=.png)
trap 'rm -f "$palette"' EXIT

for try in "960 15" "800 15" "800 12" "720 12" "640 10"; do
  read -r width fps <<<"$try"
  filters="fps=$fps,scale=$width:-1:flags=lanczos"
  ffmpeg -loglevel error -y -ss "$start" -t "$duration" -i "$input" \
    -vf "$filters,palettegen=stats_mode=diff" "$palette"
  ffmpeg -loglevel error -y -ss "$start" -t "$duration" -i "$input" -i "$palette" \
    -lavfi "$filters[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" \
    -loop 0 "$out"
  size=$(stat -c %s "$out")
  echo "$out: ${width}px, $fps fps, $((size / 1024)) KiB"
  if ((size <= limit)); then exit 0; fi
done
echo "still over 5 MB: record a smaller browser window" >&2
exit 1
