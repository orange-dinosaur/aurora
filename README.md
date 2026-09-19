# Aurora

Aurora is a desktop app for writers, to organize ideas and write stories. A project is a plain folder of Markdown files on your computer, so your writing is never locked inside the app.

It's a personal project I'm building with Tauri, React and Rust.

## Download

Get the latest version from the [Releases](https://github.com/orange-dinosaur/aurora/releases/latest) page:

- **Linux:** `.rpm` for Fedora, `.deb` for Debian and Ubuntu
- **macOS:** the `.dmg`, for both Apple Silicon and Intel Macs
- **Windows:** the `.exe` (I haven't tried this one on a real PC yet)

The apps aren't signed, so the first launch needs an extra click:

- **macOS:** open Aurora, close the warning, then go to System Settings → Privacy & Security and click **Open Anyway**. The first launch after that can take several minutes while macOS scans the app, with the icon bouncing and no window. Later launches are immediate.
- **Windows:** click **More info**, then **Run anyway**.

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
