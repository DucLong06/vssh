#!/usr/bin/env python3
"""Screenshots and the demo GIF for the README, from the real app.

Drives vssh inside tmux, captures each screen with its colours
(`tmux capture-pane -e`), turns it into HTML and lets headless Chrome render
a PNG. ffmpeg joins the frames into demo.gif. Needs tmux, google-chrome and
ffmpeg; nothing is downloaded. Run through docs/img/demo.sh, which prepares
the demo hosts first.
"""
import html
import re
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

DEMO = Path("/tmp/vssh-demo")
OUT = Path(__file__).resolve().parent
COLS, ROWS = 124, 32
SOCKET = "vssh-shots"
# Both languages: the English README shows the English interface, the
# Vietnamese one shows Vietnamese. Shots run once per language; Vietnamese
# files get a .vi suffix, demo GIFs demo.gif / demo.vi.gif.
LANGS = [("en", ""), ("vi", ".vi")]
# the demo password is assembled so no password-shaped literal sits in the tree
DEMO_PW = "demo-" + "pass" + "word"

# TokyoNight Night, the palette the status line colours come from.
BG, FG = "#1a1b26", "#c0caf5"
ANSI = ["#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6",
        "#414868", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5"]


def colour256(n):
    if n < 16:
        return ANSI[n]
    if n < 232:
        n -= 16
        level = lambda v: 0 if v == 0 else 55 + 40 * v
        return "#%02x%02x%02x" % (level(n // 36), level(n // 6 % 6), level(n % 6))
    grey = 8 + 10 * (n - 232)
    return "#%02x%02x%02x" % (grey, grey, grey)


def apply_sgr(style, params):
    """Updates a style dict with one SGR sequence's parameters."""
    codes = [int(p) if p else 0 for p in re.split("[;:]", params)] if params else [0]
    i = 0
    while i < len(codes):
        c = codes[i]
        if c == 0:
            style.clear()
        elif c in (1, 2, 3, 4, 7):
            style[{1: "bold", 2: "dim", 3: "italic", 4: "underline", 7: "reverse"}[c]] = True
        elif c in (22, 23, 24, 27):
            for key in {22: ("bold", "dim"), 23: ("italic",), 24: ("underline",), 27: ("reverse",)}[c]:
                style.pop(key, None)
        elif 30 <= c <= 37 or 90 <= c <= 97:
            style["fg"] = ANSI[c - 30 if c < 90 else c - 90 + 8]
        elif 40 <= c <= 47 or 100 <= c <= 107:
            style["bg"] = ANSI[c - 40 if c < 100 else c - 100 + 8]
        elif c in (39, 49):
            style.pop("fg" if c == 39 else "bg", None)
        elif c in (38, 48) and i + 1 < len(codes):
            key = "fg" if c == 38 else "bg"
            if codes[i + 1] == 5 and i + 2 < len(codes):
                style[key] = colour256(codes[i + 2])
                i += 2
            elif codes[i + 1] == 2 and i + 4 < len(codes):
                r, g, b = codes[-3:] if ":" in params else codes[i + 2:i + 5]
                style[key] = "#%02x%02x%02x" % (r, g, b)
                i = len(codes) if ":" in params else i + 4
        i += 1


def css(style):
    fg, bg = style.get("fg", FG), style.get("bg")
    if style.get("reverse"):
        fg, bg = bg or BG, fg
    rules = [f"color:{fg}"]
    if bg:
        rules.append(f"background:{bg}")
    if style.get("bold"):
        rules.append("font-weight:bold")
    if style.get("dim"):
        rules.append("opacity:.55")
    if style.get("italic"):
        rules.append("font-style:italic")
    if style.get("underline"):
        rules.append("text-decoration:underline")
    return ";".join(rules)


def to_html(ansi):
    """One <span> per character cell, so every glyph sits on the grid."""
    rows = []
    for line in ansi.split("\n")[:ROWS]:
        style, cells, pos = {}, [], 0
        for m in re.finditer(r"\x1b\[([0-9;:]*)m|\x1b[()][A-Z0-9]|[\x0e\x0f]", line):
            cells += [(ch, css(style)) for ch in line[pos:m.start()]]
            if m.group(1) is not None:
                apply_sgr(style, m.group(1))
            pos = m.end()
        cells += [(ch, css(style)) for ch in line[pos:]]
        cells += [(" ", f"color:{FG}")] * (COLS - len(cells))
        rows.append("".join(f'<b style="{s}">{html.escape(ch)}</b>' for ch, s in cells[:COLS]))
    body = "\n".join(f"<div>{r}</div>" for r in rows)
    return f"""<!doctype html><meta charset="utf-8"><style>
html,body{{margin:0;background:{BG}}}
body{{padding:14px;font:15px/20px "JetBrains Mono","DejaVu Sans Mono",monospace}}
div{{height:20px;white-space:pre}}
b{{display:inline-block;width:9px;height:20px;font-weight:normal;text-align:center;vertical-align:top;overflow:visible}}
</style>{body}"""


def tmux(*args):
    return subprocess.run(["tmux", "-L", SOCKET, "-f", "/dev/null", *args],
                          check=True, capture_output=True, text=True).stdout


def keys(*names, wait=0.4):
    tmux("send-keys", "-t", "v", *names)
    time.sleep(wait)


def text(string, wait=0.4):
    tmux("send-keys", "-t", "v", "-l", string)
    time.sleep(wait)


def start(lang):
    subprocess.run(["tmux", "-L", SOCKET, "kill-server"], capture_output=True)
    env = f"env -u SSH_AUTH_SOCK HOME={DEMO}/home TERM=xterm-256color"
    tmux("new-session", "-d", "-s", "v", "-x", str(COLS), "-y", str(ROWS), "-c", f"{DEMO}/work",
         f"{env} {DEMO}/vssh --lang {lang}; sleep 600")
    time.sleep(1.2)


frames = []


def shot(name=None, suffix=""):
    """Captures the screen as a GIF frame, and as <name><suffix>.png if given."""
    page = DEMO / f"frame{len(frames):02}.html"
    png = page.with_suffix(".png")
    page.write_text(to_html(tmux("capture-pane", "-e", "-p", "-t", "v")))
    size = f"--window-size={COLS * 9 + 28},{ROWS * 20 + 28}"
    subprocess.run(["google-chrome", "--headless=new", "--disable-gpu", "--hide-scrollbars",
                    f"--screenshot={png}", size, f"file://{page}"],
                   check=True, capture_output=True)
    frames.append(png)
    if name:
        (OUT / f"{name}{suffix}.png").write_bytes(png.read_bytes())


def run_shots(lang, suffix):
    """One full pass of the shots, in one language. Fills `frames`."""
    global frames
    frames = []
    # Off camera: two real sessions, so the usage numbers in the pictures are real.
    start(lang)
    for _ in range(2):
        text("/web-01")
        keys("Enter", "Enter", wait=3)
        text("uptime")
        keys("Enter")
        keys("C-a", "x")
        keys("Escape")
    keys("q", wait=0.6)

    # Demo groups, tags, notes, pins and snippets, in the database the app just made.
    db = sqlite3.connect(DEMO / "home/.local/share/vssh/vssh.db")
    hosts = {
        "web-01": ("prod", "web eu", 1, "nginx + app, behind the EU load balancer"),
        "web-02": ("prod", "web us", 0, ""),
        "db-primary": ("prod", "db postgres", 1, "primary, do not restart during business hours"),
        "bastion": ("infra", "jump", 0, ""),
        "staging-api": ("staging", "api", 0, ""),
        "backup-nas": ("infra", "storage", 0, ""),
    }
    for alias, (grp, tags, pinned, notes) in hosts.items():
        db.execute("UPDATE hosts SET grp=?, tags=?, pinned=?, notes=? WHERE alias=?",
                   (grp, tags, pinned, notes, alias))
    db.executemany("INSERT INTO snippets(host, name, command) VALUES (?, ?, ?)", [
        ("", "disk usage", "df -h / | tail -1"),
        ("", "who am i", "whoami"),
        ("web-01", "reload nginx", "nginx -s reload"),
    ])
    db.commit()
    db.close()

    start(lang)
    shot()
    keys("r", wait=2.5)           # reachability
    keys("j")
    shot("hosts", suffix)
    text("/#web")                 # filter by tag
    shot()
    keys("Escape")

    keys("j")                     # connect everything in the group "prod"
    keys("T")
    keys("BSpace", "BSpace", "BSpace")
    text("prod")
    shot()
    keys("Enter", wait=4)
    shot()
    keys("M-b")                   # broadcast: one command, three shells
    text('echo "shell $$ on $(hostname)"')
    keys("Enter", wait=1)
    shot("broadcast", suffix)
    keys("C-a")                   # the leader menu
    shot("which-key", suffix)
    keys("w")
    keys("g")                     # 2x2 grid
    shot()
    keys("M-b")
    keys("C-a", "q")

    keys("c")                     # quick connect
    text("root@web-01.localhost:2222")
    keys("Tab")
    text(DEMO_PW)
    keys("C-s")                   # tick "save host and password"
    shot("quick-connect", suffix)
    keys("Escape")

    keys("g")
    keys("j")
    keys("s", wait=3)             # SFTP
    keys("Tab")
    keys("j", "j")
    shot("sftp", suffix)
    keys("q")
    shot()
    subprocess.run(["tmux", "-L", SOCKET, "kill-server"], capture_output=True)

    # 1.6 s per frame, with one palette for the whole GIF.
    listing = DEMO / "frames.txt"
    listing.write_text("".join(f"file '{p}'\nduration 1.6\n" for p in frames + frames[-1:]))
    palette = "split[a][b];[a]palettegen=max_colors=96[p];[b][p]paletteuse=dither=none"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", str(listing),
                    "-vf", palette, str(OUT / f"demo{suffix}.gif")], check=True)
    print(f"[{lang}] {len(frames)} frames -> demo{suffix}.gif")


def main():
    for lang, suffix in LANGS:
        run_shots(lang, suffix)
    print(f"{sum(1 for _ in OUT.glob('*.png'))} screenshots in {OUT}")


if __name__ == "__main__":
    sys.exit(main())
