"""Turn a tmux pane capture (with ANSI colors) into an HTML page of a terminal window.

usage: render.py CAPTURE OUT_HTML THEME_TOML TITLE COLUMNS

run.sh photographs the page with headless Chromium. Colors come from an
Omarchy-style colors.toml, so ANSI palette colors match the true colors drift
emits for the same theme.

Colon-separated SGR subparameters (38:2::r:g:b) are not supported; tmux
writes the semicolon form.
"""

import html
import re
import sys
import tomllib

ANSI_NAMES = ("black", "red", "green", "yellow", "blue", "magenta", "cyan", "white")
FULL_BLOCK = "█"
SGR = re.compile(r"\x1b\[([0-9;:]*)m")


def palette(theme):
    fg = theme["foreground"]
    normal = [theme.get(name, fg) for name in ANSI_NAMES]
    normal[0] = theme.get("color0", theme.get("dark_background", "#000000"))
    bright = [theme.get("bright_" + name, normal[i]) for i, name in enumerate(ANSI_NAMES)]
    bright[0] = theme.get("color8", theme.get("muted", "#555555"))
    return normal + bright


def xterm256(n, base16):
    if n < 16:
        return base16[n]
    if n < 232:
        n -= 16
        level = lambda v: 0 if v == 0 else 55 + v * 40
        return "#%02x%02x%02x" % (level(n // 36), level(n // 6 % 6), level(n % 6))
    grey = 8 + (n - 232) * 10
    return "#%02x%02x%02x" % (grey, grey, grey)


class Style:
    def __init__(self):
        self.reset()

    def reset(self):
        self.fg = self.bg = None
        self.bold = self.dim = self.italic = self.underline = self.reverse = False

    def apply(self, params, base16):
        k = 0
        while k < len(params):
            p = params[k]
            if p == 0:
                self.reset()
            elif p == 1:
                self.bold = True
            elif p == 2:
                self.dim = True
            elif p == 3:
                self.italic = True
            elif p == 4:
                self.underline = True
            elif p == 7:
                self.reverse = True
            elif p == 22:
                self.bold = self.dim = False
            elif p == 23:
                self.italic = False
            elif p == 24:
                self.underline = False
            elif p == 27:
                self.reverse = False
            elif 30 <= p <= 37:
                self.fg = base16[p - 30]
            elif 90 <= p <= 97:
                self.fg = base16[p - 82]
            elif 40 <= p <= 47:
                self.bg = base16[p - 40]
            elif 100 <= p <= 107:
                self.bg = base16[p - 92]
            elif p == 39:
                self.fg = None
            elif p == 49:
                self.bg = None
            elif p in (38, 48) and k + 1 < len(params):
                if params[k + 1] == 5 and k + 2 < len(params):
                    color = xterm256(params[k + 2], base16)
                    k += 2
                elif params[k + 1] == 2 and k + 4 < len(params):
                    color = "#%02x%02x%02x" % tuple(params[k + 2 : k + 5])
                    k += 4
                else:
                    color = None
                if p == 38:
                    self.fg = color
                else:
                    self.bg = color
            k += 1

    def css(self, default_fg, default_bg, block=False):
        fg, bg = self.fg, self.bg
        if self.reverse:
            fg, bg = bg or default_bg, fg or default_fg
        if block:
            # Full blocks become background-colored spaces: glyphs leave seams
            # between cells, a background does not.
            bg = fg or default_fg
        rules = []
        if fg:
            rules.append("color:" + fg)
        if bg:
            rules.append("background:" + bg)
        if self.bold:
            rules.append("font-weight:700")
        if self.dim:
            rules.append("opacity:.6")
        if self.italic:
            rules.append("font-style:italic")
        if self.underline:
            rules.append("text-decoration:underline")
        return ";".join(rules)


def render_line(line, style, columns, base16, fg, bg):
    spans, pos = [], 0

    def emit(text):
        for chunk in re.split(f"({FULL_BLOCK}+)", text):
            if not chunk:
                continue
            block = chunk[0] == FULL_BLOCK
            css = style.css(fg, bg, block)
            chunk = " " * len(chunk) if block else chunk
            if spans and spans[-1][0] == css:
                spans[-1][1] += chunk
            else:
                spans.append([css, chunk])

    for match in SGR.finditer(line):
        emit(line[pos : match.start()])
        pos = match.end()
        params = [int(p) if p else 0 for p in re.split("[;:]", match.group(1))]
        style.apply(params, base16)
    emit(line[pos:])

    # tmux drops trailing blanks; pad so centered screens stay centered.
    visible = len(SGR.sub("", line))
    if visible < columns:
        spans.append(["", " " * (columns - visible)])
    body = "".join(f'<span style="{css}">{html.escape(text)}</span>' for css, text in spans)
    return f'<div class="row">{body}</div>'


def main():
    capture, out, theme_file, title, columns = sys.argv[1:6]
    with open(theme_file, "rb") as f:
        theme = tomllib.load(f)
    fg, bg = theme["foreground"], theme["background"]
    base16 = palette(theme)
    with open(capture, encoding="utf-8") as f:
        lines = f.read().rstrip("\n").split("\n")
    # tmux omits SGR codes a row inherits from the previous one, so the
    # style carries over from row to row.
    style = Style()
    rows = "".join(render_line(line, style, int(columns), base16, fg, bg) for line in lines)
    bar = theme.get("dark_background", bg)
    title_color = theme.get("light_foreground", fg)
    page = f"""<!doctype html><html><head><meta charset="utf-8"><style>
@font-face {{ font-family: Term; src: local("JetBrainsMono Nerd Font Mono"), local("JetBrainsMono NF"), local("JetBrains Mono"); }}
html, body {{ margin: 0; background: transparent; }}
.wrap {{ padding: 28px; display: inline-block; }}
.win {{ background: {bg}; border-radius: 10px; overflow: hidden;
  box-shadow: 0 14px 40px rgba(0,0,0,.45), 0 0 0 1px rgba(255,255,255,.06); }}
.bar {{ height: 34px; display: flex; align-items: center; gap: 8px; padding: 0 14px; background: {bar}; }}
.dot {{ width: 12px; height: 12px; border-radius: 50%; }}
.title {{ flex: 1; text-align: center; margin-right: 52px; font: 500 13px system-ui, sans-serif; color: {title_color}; }}
pre {{ margin: 0; padding: 14px 18px 16px; font: 14px/1 Term, monospace; white-space: pre;
  color: {fg}; font-variant-ligatures: none; }}
.row {{ height: 18px; line-height: 18px; overflow: hidden; }}
</style></head><body><div class="wrap"><div class="win">
<div class="bar"><span class="dot" style="background:#ff5f57"></span><span class="dot" style="background:#febc2e"></span><span class="dot" style="background:#28c840"></span><span class="title">{html.escape(title)}</span></div>
<pre>{rows}</pre></div></div></body></html>"""
    with open(out, "w", encoding="utf-8") as f:
        f.write(page)


if __name__ == "__main__":
    main()
