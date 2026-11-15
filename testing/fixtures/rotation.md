Videos with a display matrix, each the unchanged streams of another fixture
with the matrix added by FFmpeg, a few kilobytes each. `-display_rotation`
counts counterclockwise.

```sh
cd testing/fixtures
# Quarter turns of H264 with AAC: video-rotate-{90,180,270}.mp4.
ffmpeg -v error -y -display_rotation:v:0 90 -i video-aac.mp4 -c copy video-rotate-90.mp4
# Flipped left to right and top to bottom, and both flips across a diagonal.
ffmpeg -v error -y -display_hflip:v:0 -i gif-h264.mp4 -c copy gif-h264-flip-h.mp4
ffmpeg -v error -y -display_vflip:v:0 -i gif-h264.mp4 -c copy gif-h264-flip-v.mp4
ffmpeg -v error -y -display_rotation:v:0 90 -display_hflip:v:0 \
  -i gif-h264.mp4 -c copy gif-h264-rotate-90-flip.mp4
ffmpeg -v error -y -display_rotation:v:0 270 -display_hflip:v:0 \
  -i gif-h264.mp4 -c copy gif-h264-rotate-270-flip.mp4
# Not a quarter turn: Yapfast shows it as it's stored.
ffmpeg -v error -y -display_rotation:v:0 45 -i gif-h264.mp4 -c copy gif-h264-rotate-45.mp4
# 10-bit HEVC on its side, as an iPhone held upright records.
ffmpeg -v error -y -display_rotation:v:0 90 \
  -i gif-hevc-10bit.mp4 -c copy gif-hevc-10bit-rotate-90.mp4
# A transparent WebM on its side: its alpha turns with it.
ffmpeg -v error -y -display_rotation:v:0 90 \
  -i gif-vp9-alpha.webm -c copy gif-vp9-alpha-rotate-90.webm
```

`ffprobe -show_entries side_data=displaymatrix` prints each matrix; the
test `applies_video_display_rotation` (`crates/yapfast-core/src/video.rs`)
lists them. It compares every pixel shown with the original's, at the
video's size, halved and at an odd size, checks the size reported and the
sound, and moves back to the start between.
