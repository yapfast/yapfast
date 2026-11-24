HEVC playback fixtures, generated with FFmpeg (libx265 and libopus):

```sh
# Main profile, hvc1 MP4 with reordered frames and eight-channel Opus,
# matching the reported ShareX codec/container combination.
ffmpeg -v error -y \
  -f lavfi -i 'testsrc2=size=160x90:rate=25:duration=2' \
  -f lavfi -i 'aevalsrc=0.1*sin(440*2*PI*t)|0|0|0|0|0|0|0:s=48000:d=2:c=7.1' \
  -c:v libx265 -preset fast \
  -x265-params 'log-level=error:pools=1:keyint=250:bframes=4' \
  -tag:v hvc1 -c:a libopus -mapping_family 1 -movflags +faststart \
  testing/fixtures/video-hevc-opus.mp4

# Main 10 profile, hev1 MP4; also checks animation and colour conversion.
ffmpeg -v error -y -i testing/fixtures/gif-h264.mp4 \
  -c:v libx265 -preset fast -pix_fmt yuv420p10le \
  -x265-params 'log-level=error:pools=1' -tag:v hev1 -an \
  testing/fixtures/gif-hevc-10bit.mp4
```
