import { useEffect, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { failure } from "./errors";
import {
	FORMATS,
	doing,
	flipped,
	fraction,
	sized,
	warned,
	wrote,
} from "./exporting";
import type { ExportFormat, ExportProgress, Extent } from "./types";

/** How long an export runs before its bar is shown, so a quick one does not
 * flash it on and off. */
const SLOW_MS = 200;

type Props = {
	root: string;
	formats: ExportFormat[];
	onFormats: (formats: ExportFormat[]) => void;
	/** Whether the book has a cover, which an EPUB wants. */
	hasCover: boolean;
	/** Saves what the page is still holding, so the export reads the book as
	 * it stands on screen rather than as it stood at the last autosave. */
	onBeforeExport: () => Promise<void>;
};

/**
 * The formats to write and the button that writes them. The folder is asked
 * for on every export rather than remembered, since where a book goes is
 * decided each time it goes somewhere.
 */
export default function Export({
	root,
	formats,
	onFormats,
	hasCover,
	onBeforeExport,
}: Props) {
	const [extent, setExtent] = useState<Extent | null>(null);
	const [busy, setBusy] = useState(false);
	const [slow, setSlow] = useState(false);
	const [progress, setProgress] = useState<ExportProgress | null>(null);
	const [said, setSaid] = useState("");

	useEffect(() => {
		let gone = false;

		invoke<Extent>("book_extent", { root })
			.then((read) => {
				if (!gone) {
					setExtent(read);
				}
			})
			.catch(() => {
				// The count is a courtesy; the export works without it.
			});

		return () => {
			gone = true;
		};
	}, [root]);

	async function run() {
		const folder = await open({
			directory: true,
			multiple: false,
			title: "Choose where the export goes",
		});

		if (typeof folder !== "string") {
			return;
		}

		// A report arriving after the export has answered is about an export
		// that is over, and must not bring the bar back.
		let over = false;
		const channel = new Channel<ExportProgress>();
		channel.onmessage = (step) => {
			if (!over) {
				setProgress(step);
			}
		};
		const timer = window.setTimeout(() => setSlow(true), SLOW_MS);

		setBusy(true);
		try {
			await onBeforeExport();
			const files = await invoke<string[]>("export_book", {
				root,
				folder,
				formats,
				progress: channel,
			});
			setSaid(wrote(files, folder));
		} catch (error) {
			setSaid(failure(error).message);
		} finally {
			over = true;
			window.clearTimeout(timer);
			setBusy(false);
			setSlow(false);
			setProgress(null);
		}
	}

	// The bar has something to show only once an export has run for a while.
	const current = busy && slow ? progress : null;
	const percent = current === null ? 0 : Math.round(fraction(current) * 100);
	// The export as it runs, what it said when it finished, or else anything
	// worth knowing before one starts.
	const line =
		current === null ? said || warned(formats, hasCover) : doing(current);

	return (
		<div className="export">
			<div className="export__formats">
				{FORMATS.map(({ key, name }) => (
					<button
						key={key}
						type="button"
						className="export__format"
						role="checkbox"
						aria-checked={formats.includes(key)}
						onClick={() => {
							// What the last export said was about the last choice.
							setSaid("");
							onFormats(flipped(formats, key));
						}}
					>
						<span className="export__tick" />
						{name}
					</button>
				))}
			</div>

			<div className="export__action">
				<button
					type="button"
					className="export__button"
					disabled={busy || formats.length === 0}
					onClick={() => void run()}
				>
					{busy ? "Exporting…" : "Export…"}
				</button>
				<p className="export__size">
					{extent === null ? "" : sized(extent)}
				</p>
			</div>

			{/* The bar and the line under it are always drawn, so neither an
			    export starting nor what it says at the end moves anything. */}
			<div
				className="export__track"
				role="progressbar"
				aria-label="Export progress"
				aria-valuemin={0}
				aria-valuemax={100}
				aria-valuenow={percent}
				data-shown={current !== null}
			>
				<div
					className="export__fill"
					style={{ width: `${percent}%` }}
				/>
			</div>

			<p className="export__said" role="status">
				{line}
			</p>
		</div>
	);
}
