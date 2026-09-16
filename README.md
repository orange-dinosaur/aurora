# Aurora

Aurora is a desktop app for writers, to organize ideas and write stories. A project is a plain folder of Markdown files on your computer, so your writing is never locked inside the app.

It's a personal project I'm building with Tauri, React and Rust.

## Download

Get the latest version from the [Releases](https://github.com/orange-dinosaur/aurora/releases/latest) page:

- **Linux:** `.rpm` for Fedora, `.deb` for Debian and Ubuntu, or the `.AppImage` for anything else
- **macOS:** the `.dmg`, for both Apple Silicon and Intel Macs
- **Windows:** the `.exe` (I haven't tried this one on a real PC yet)

The apps aren't signed, so the first launch needs an extra click:

- **macOS:** open Aurora, close the warning, then go to System Settings → Privacy & Security and click **Open Anyway**.
- **Windows:** click **More info**, then **Run anyway**.
- **AppImage:** make it executable first with `chmod +x Aurora_*.AppImage`.

Aurora checks for updates when it starts and asks before installing anything. You can turn this off in Settings.

## Building it yourself

You need the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) and pnpm.

```sh
pnpm install
pnpm tauri dev
```

## License

Aurora is licensed under the GPL-3.0-or-later, see [LICENSE](LICENSE). Copyright © 2026 orange-dinosaur.

The fonts in `src/assets/fonts` (Newsreader, Spectral, Source Serif 4, IBM Plex Sans and IBM Plex Mono) are under the SIL Open Font License.
