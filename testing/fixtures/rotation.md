`video-rotate-{90,180,270}.mp4` contain the unchanged H264/AAC streams from
`video-aac.mp4`, with a display matrix added by FFmpeg. For each angle:

```sh
ffmpeg -v error -y -display_rotation:v:0 90 \
  -i testing/fixtures/video-aac.mp4 -c copy testing/fixtures/video-rotate-90.mp4
```

The tests compare every output pixel against the original frame at native
and thumbnail sizes, check display dimensions and audio, and seek back.
