# Demo video

Generate `.github/assets/demo/drift-teaser.mp4` and `poster.png`:

```bash
# Fetch the test server image if it is not already installed.
docker pull atmoz/sftp:alpine
python3 scripts/demo-video/run.py
```

Requires Python 3.11+, Go, Git, Docker, tmux, Chromium, FFmpeg (with libx264),
and JetBrains Mono (the Nerd Font variant is preferred). Docker must be running.
No Python packages are needed.

The 27-second teaser shows the real application selecting a file, displaying its
diff, uploading it, and reporting no remaining differences. It has English text
and key overlays, no audio, and exports H.264 at 1920 × 1080, 30 fps.

The script builds the current checkout and reuses `scripts/screenshots/seed.sh`,
its theme, and its ANSI renderer. It captures the running tmux pane at 10 Hz,
renders distinct frames with Chromium, and encodes them with FFmpeg. The output
is a terminal recording with added captions, not a desktop screen capture.

All project files, credentials, configuration, SSH trust, and caches belong to a
temporary demo home. The SFTP container listens only on a random loopback port;
the illustrative production host from the screenshot seed is removed. The script
cleans up its container, tmux server, and temporary directory when it exits.

Before exporting, it checks the selection, diff, and completion screens, compares
the uploaded bytes, and verifies that every other remote file is unchanged. It
also decodes the completed MP4 to check for encoding errors. If a UI change breaks
the scripted key sequence, the run fails rather than replacing the output.

The main timings and captions are in `actions` and `scenes` in `run.py`. The MP4
has fast-start metadata for web playback; `poster.png` is a still from its diff
scene. The script does not upload or publish either file.
