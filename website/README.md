# drift website

A single-page, dependency-free HTML/CSS site. Its copy follows the project README, and the screenshot comes from `.github/assets/screenshots/drift-browser.png`.

Serve this directory with any static web server. For a local preview from the repository root:

```sh
python3 -m http.server 8080 --directory website --bind 127.0.0.1
```

Open http://127.0.0.1:8080. No build step is required. Deploy the contents of `website/` to a static host. Keep the screenshot in `assets/` when deploying.

The terminal in the hero is an illustrative example, not a live connection. Installation commands are selectable text. Navigation, documentation, source, and issue links work without JavaScript.
