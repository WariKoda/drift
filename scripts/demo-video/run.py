#!/usr/bin/env python3
"""Record the real drift TUI against an isolated, loopback-only SFTP server."""

import concurrent.futures
import hashlib
import html
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile
import time
import tomllib

REPO = Path(__file__).resolve().parents[2]
KIT = REPO / "scripts/screenshots"
OUT = REPO / ".github/assets/demo/drift-teaser.mp4"
COLS, ROWS, FPS = 140, 30, 10


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, capture_output=True, **kwargs).stdout


def main():
    for tool in ("go", "git", "docker", "tmux", "chromium", "ffmpeg", "ffprobe"):
        if not shutil.which(tool):
            raise SystemExit(f"Missing dependency: {tool}")
    run("docker", "image", "inspect", "atmoz/sftp:alpine")
    renderer = runpy.run_path(str(KIT / "render.py"))
    theme = tomllib.loads((KIT / "theme.toml").read_text())
    with tempfile.TemporaryDirectory(prefix="drift-video-") as temp:
        work = Path(temp)
        state = work / ".state"
        state.mkdir()
        binary = work / "drift"
        name = work.name
        tmux = ("tmux", "-L", name)
        run("go", "build", "-o", str(binary), ".", cwd=REPO)
        run("bash", str(KIT / "seed.sh"), str(state), str(binary), "0")
        home = state / "home"
        project = home / "work/acme-shop"
        remote = state / "remote/acme-shop"
        before = {p.relative_to(remote): p.read_bytes() for p in remote.rglob("*") if p.is_file()}
        try:
            run("docker", "run", "-d", "--name", name,
                "-p", "127.0.0.1::22", "-v", f"{state / 'remote'}:/home/drift/upload",
                "atmoz/sftp:alpine", f"drift:secret:{os.getuid()}::upload")
            port = run("docker", "port", name, "22/tcp").strip().rsplit(":", 1)[1]
            # Remove the seed's illustrative production host: this run has one target.
            store = home / ".config/drift/projects/acme-shop.toml"
            host = store.read_text().split("\n[[hosts]]")[0]
            store.write_text(host.replace("port = 0", f"port = {port}"))
            env = {k: v for k, v in os.environ.items() if not k.startswith("DRIFT_")}
            env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"),
                       XDG_CACHE_HOME=str(home / ".cache"),
                       XDG_STATE_HOME=str(home / ".local/state"),
                       TERM="xterm-256color", COLORTERM="truecolor",
                       DRIFT_THEME="omarchy", DRIFT_THEME_FILE=str(KIT / "theme.toml"))
            time.sleep(2)
            run(*tmux, "-f", "/dev/null", "new-session", "-d", "-s", "demo",
                "-x", str(COLS), "-y", str(ROWS), "-c", str(project),
                str(binary), "--no-dashboard", env=env)

            def keys(*items):
                for key in items:
                    run(*tmux, "send-keys", "-t", "demo", key)
                    time.sleep(0.18)

            def wait_for(text):
                deadline = time.monotonic() + 12
                while time.monotonic() < deadline:
                    pane = run(*tmux, "capture-pane", "-t", "demo", "-p")
                    if text in pane:
                        return
                    time.sleep(0.1)
                raise RuntimeError(f"Expected {text!r} on screen:\n{pane}")

            wait_for("LOCAL")
            keys("@", "Enter")
            wait_for("Remote connected: staging")
            keys("j", "j", "l", "j", "j", "l", "j", "j")
            keys("Tab", "j", "j", "l", "j", "j", "l", "j", "Tab")
            wait_for("PriceCalculator.php")

            # Times are measured against the live session, including network work.
            actions = {30: "Space", 60: "s", 110: "j", 114: "j", 118: "j",
                       122: "j", 126: "j", 130: "j", 160: "Home", 180: "s"}
            scenes = [
                (0, "01 / BROWSE", "Browse local and remote files side by side.", ""),
                (30, "02 / SELECT", "Select the file you want to compare.", "Space · select"),
                (60, "03 / REVIEW", "See exactly what changed before you upload.", "s · compare"),
                (110, "03 / REVIEW", "Read the diff. Keep control of every change.", "↓ · scroll"),
                (160, "04 / SYNC", "Upload only the selected file.", "s · sync file"),
                (200, "05 / VERIFIED", "Local and remote match. Done.", ""),
                (230, "github.com/WariKoda/drift", "Review first. Sync only what you approve.", "SFTP · FTP · FTPS"),
            ]
            captures, sequence = {}, []
            start = time.monotonic()
            for tick in range(270):
                time.sleep(max(0, start + tick / FPS - time.monotonic()))
                if tick in actions:
                    run(*tmux, "send-keys", "-t", "demo", actions[tick])
                pane = run(*tmux, "capture-pane", "-t", "demo", "-e", "-p")
                expected = {40: "1 marked", 85: "+11 -5", 205: "No differences"}.get(tick)
                if expected and expected not in pane:
                    raise RuntimeError(f"Recording missed {expected!r} at {tick / FPS}s")
                scene = next(scene for scene in reversed(scenes) if tick >= scene[0])
                digest = hashlib.sha256((pane + repr(scene)).encode()).hexdigest()[:16]
                captures[digest] = (pane, scene)
                sequence.append(digest)
            wait_for("No differences")
            target = Path("src/Service/PriceCalculator.php")
            if (project / target).read_bytes() != (remote / target).read_bytes():
                raise RuntimeError("Uploaded file does not match the local file")
            after = {p.relative_to(remote): p.read_bytes() for p in remote.rglob("*") if p.is_file()}
            if before.keys() != after.keys():
                raise RuntimeError("Sync unexpectedly added or removed remote files")
            for path, content in before.items():
                if path != target and content != after[path]:
                    raise RuntimeError(f"Unexpected change to unselected file: {path}")
            print(f"Captured 27 seconds, {len(captures)} distinct frames; upload verified.")
        finally:
            subprocess.run((*tmux, "kill-server"), capture_output=True)
            subprocess.run(("docker", "rm", "-f", name), capture_output=True)

        def render(item):
            digest, (pane, (_, stage, caption, badge)) = item
            style = renderer["Style"]()
            rows = "".join(renderer["render_line"](
                line, style, COLS, renderer["palette"](theme),
                theme["foreground"], theme["background"])
                for line in pane.rstrip("\n").split("\n"))
            page = work / f"{digest}.html"
            png = work / f"{digest}.png"
            page.write_text(f'''<!doctype html><html><meta charset="utf-8"><style>
@font-face {{ font-family: Term; src: local("JetBrainsMono Nerd Font"), local("JetBrains Mono"); }}
* {{ box-sizing: border-box; }}
body {{ margin:0; width:1920px; height:1080px; overflow:hidden; background:#0e0e14;
    color:#c0caf5; font-family:system-ui,sans-serif; }}
header {{ position:absolute; left:98px; right:98px; top:35px; display:flex;
    align-items:center; justify-content:space-between; }}
.logo {{ font:700 44px Term,monospace; color:#7aa2f7; }}
.stage {{ font-size:22px; color:#a9b1d6; }}
.win {{ position:absolute; top:124px; left:50%; transform:translateX(-50%);
    background:#1a1b26; border:1px solid #292e42; border-radius:12px;
    overflow:hidden; box-shadow:0 20px 50px #0006; }}
.bar {{ height:40px; background:#13141c; padding:8px 20px; color:#a9b1d6;
    font:16px system-ui,sans-serif; }}
pre {{ margin:0; padding:18px 20px; font:20px/24px Term,monospace;
    white-space:pre; font-variant-ligatures:none; color:#a9b1d6; }}
.row {{ height:24px; line-height:24px; overflow:hidden; }}
footer {{ position:absolute; left:98px; right:98px; top:967px; display:flex;
    align-items:center; justify-content:space-between; font-size:30px; }}
.badge {{ font:20px Term,monospace; color:#7aa2f7; padding:12px 18px;
    border:1px solid #414868; border-radius:8px; }}
</style><header><div class="logo">drift</div><div class="stage">{html.escape(stage)}</div></header>
<div class="win"><div class="bar">Acme Shop · staging</div><pre>{rows}</pre></div>
<footer><span>{html.escape(caption)}</span>{f'<span class="badge">{html.escape(badge)}</span>' if badge else ''}</footer></html>''')
            run("chromium", "--headless=new", "--disable-gpu", "--hide-scrollbars",
                "--run-all-compositor-stages-before-draw",
                "--no-first-run", "--no-default-browser-check",
                f"--user-data-dir={work / ('chrome-' + digest)}",
                "--force-device-scale-factor=1", "--window-size=1920,1080",
                "--virtual-time-budget=1000", f"--screenshot={png}", page.as_uri())
            shutil.rmtree(work / ("chrome-" + digest))
            return png

        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
            list(pool.map(render, captures.items()))
        manifest = work / "frames.txt"
        entries = []
        for digest in sequence:
            entries.extend((f"file '{work / (digest + '.png')}'", f"duration {1 / FPS}"))
        entries.append(f"file '{work / (sequence[-1] + '.png')}'")
        manifest.write_text("\n".join(entries) + "\n")
        OUT.parent.mkdir(parents=True, exist_ok=True)
        movie = work / "teaser.mp4"
        run("ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "concat",
            "-safe", "0", "-i", str(manifest), "-t", "27", "-r", "30",
            "-vf", "fade=t=in:st=0:d=0.3,fade=t=out:st=26.6:d=0.4",
            "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p",
            "-movflags", "+faststart", str(movie))
        run("ffmpeg", "-v", "error", "-i", str(movie), "-f", "null", "-")
        shutil.copyfile(movie, OUT)
        shutil.copyfile(work / (sequence[85] + ".png"), OUT.parent / "poster.png")
        print(run("ffprobe", "-v", "error", "-show_entries",
                  "stream=codec_name,width,height,r_frame_rate:format=duration,size",
                  "-of", "json", str(OUT)))
        print(f"Wrote {OUT}")


if __name__ == "__main__":
    main()
