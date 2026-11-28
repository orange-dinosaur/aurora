# Aurora

Aurora is a desktop app for writers, to organize ideas and write stories. A project is a plain folder of Markdown files on your computer, so your writing is never locked inside the app.

It's a personal project I'm building with Tauri, React and Rust.

## What it does

You start on a shelf of your books. Each one has a cover and a tag saying whether it's a novel, a screenplay or a collection of short stories. You can filter the shelf by that tag, search it by title or author, and sort it. If you'd rather see a table, there's a list view.

Inside a book, the binder on the left holds everything. The manuscript is split into parts, chapters and scenes. Next to it sit an outline, your characters and your locations. Every folder shows its word count, and if you give it a target, a small ring fills up as you write.

A new novel can start from a structure like Three acts, which gives you parts, chapters and beats to fill in. Or you can start from a single empty scene and let the shape come later.

A few more things it does:

- Aurora notices when a character's name turns up in a scene. You don't tag anything, you just write.
- The timeline lays out your scenes in story order, one row per character, so you can see who is where. Drag a scene to move it. Markers name stretches of time, like "Three years later".
- Writing sessions count the words you write and delete. A sprint is a session with a limit in minutes or words.
- Takes let you rewrite a scene another way without losing the first try. You pick which take counts.
- Versions are for bigger changes. You can try a rewrite of the whole book, compare it with Main, and merge back what you like. Checkpoints save the project as it stood, with a note on what changed.
- Screenplays are written in Fountain, the plain-text screenplay format.
- Export writes the whole book out as Word, EPUB, Kobo EPUB, PDF, Markdown or plain text. A screenplay can also go out as Fountain or Final Draft.

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

The fonts in `src/assets/fonts` (Instrument Sans, Instrument Serif, Newsreader, Spectral, Source Serif 4, IBM Plex Sans and IBM Plex Mono) are under the SIL Open Font License.
