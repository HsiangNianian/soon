#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
output_dir="$script_dir/rendered"
render_tmp=$(mktemp -d "${TMPDIR:-/tmp}/soon-launch.XXXXXX")

cleanup() {
  rm -rf -- "$render_tmp"
}
trap cleanup EXIT

for tool in rsvg-convert ffmpeg ffprobe; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing required tool: $tool" >&2
    exit 1
  fi
done

mkdir -p "$output_dir"

rsvg-convert --width 1280 --height 720 \
  "$script_dir/repair-receipt-1280x720.svg" \
  --output "$output_dir/repair-receipt-1280x720.png"
rsvg-convert --width 1080 --height 1080 \
  "$script_dir/terminal-proof-1080x1080.svg" \
  --output "$output_dir/terminal-proof-1080x1080.png"
rsvg-convert --width 1500 --height 500 \
  "$script_dir/workflow-strip-1500x500.svg" \
  --output "$output_dir/workflow-strip-1500x500.png"

rsvg-convert --width 4800 --height 675 \
  "$script_dir/demo-frames.svg" \
  --output "$render_tmp/demo-frames.png"

for frame in 0 1 2 3; do
  x_offset=$((frame * 1200))
  ffmpeg -hide_banner -loglevel error -y \
    -i "$render_tmp/demo-frames.png" \
    -vf "crop=1200:675:${x_offset}:0" \
    "$render_tmp/frame-$((frame + 1)).png"
done

ffmpeg -hide_banner -loglevel error -y \
  -loop 1 -t 3.975 -i "$render_tmp/frame-1.png" \
  -loop 1 -t 3.975 -i "$render_tmp/frame-2.png" \
  -loop 1 -t 3.975 -i "$render_tmp/frame-3.png" \
  -loop 1 -t 3.975 -i "$render_tmp/frame-4.png" \
  -filter_complex \
    "[0:v]format=rgba,setsar=1[v0]; \
     [1:v]format=rgba,setsar=1[v1]; \
     [2:v]format=rgba,setsar=1[v2]; \
     [3:v]format=rgba,setsar=1[v3]; \
     [v0][v1]xfade=transition=fade:duration=0.3:offset=3.675[x1]; \
     [x1][v2]xfade=transition=fade:duration=0.3:offset=7.35[x2]; \
     [x2][v3]xfade=transition=fade:duration=0.3:offset=11.025,scale=1280:720:flags=lanczos,format=yuv420p[v]" \
  -map "[v]" -t 15 -r 30 -an -movflags +faststart \
  "$output_dir/soon-v0.4.1-demo-1280x720.mp4"

ffmpeg -hide_banner -loglevel error -y \
  -i "$output_dir/soon-v0.4.1-demo-1280x720.mp4" \
  -filter_complex \
    "[0:v]fps=12,scale=960:-1:flags=lanczos,split[gif][palette]; \
     [palette]palettegen=max_colors=128:stats_mode=diff[p]; \
     [gif][p]paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle" \
  -loop 0 "$output_dir/soon-v0.4.1-demo-960x540.gif"

video_duration=$(ffprobe -v error -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 \
  "$output_dir/soon-v0.4.1-demo-1280x720.mp4")

python3 - "$video_duration" <<'PY'
import sys

duration = float(sys.argv[1])
if not 14.95 <= duration <= 15.05:
    raise SystemExit(f"unexpected video duration: {duration:.3f}s")
PY

echo "rendered launch assets in $output_dir (${video_duration}s demo)"
