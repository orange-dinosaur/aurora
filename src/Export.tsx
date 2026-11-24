import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { failure } from "./errors";
import Dialog from "./Dialog";
import { DIALOG_FADE, useLingering } from "./Menu";
import {
	doing,
	flipped,
	fraction,
	offered,
	sized,
	warned,
	wrote,
} from "./exporting";
import { useFormat } from "./projectFormat";
import type { ExportFormat, ExportProgress, Extent } from "./types";

/** How long an export runs before its bar is shown, so a quick one does not
 * flash it on and off. */
const SLOW_MS = 200;

type Props = {
	root: string;
	/** Goes up whenever a document or the manifest is written, so the count
	 * is read again when a scene is switched out of the book or moved. */
	changed: number;
	formats: ExportFormat[];
	onFormats: (formats: ExportFormat[]) => void;
	/** Whether the book has a cover, which an EPUB wants. */
	hasCover: boolean;
	/** Saves what the page is still holding, so the export reads the book as
	 * it stands on screen rather than as it stood at the last autosave. */
	onBeforeExport: () => Promise<void>;
};

/**
 * The button that opens the Export dialog, and the dialog: the formats to write
 * and the button that writes them. The folder is asked for on every export
 * rather than remembered, since where a book goes is decided each time it goes
 * somewhere.
 */
export default function Export({
	root,
	changed,
	formats: ticks,
	onFormats,
	hasCover,
	onBeforeExport,
}: Props) {
	const shown = offered(useFormat());
	// A tick this project cannot export, from a manifest edited by hand, is
	// neither drawn nor sent.
	const formats = ticks.filter((tick) =>
		shown.some((offer) => offer.key === tick),
	);
	const [extent, setExtent] = useState<Extent | null>(null);
	const [busy, setBusy] = useState(false);
	const [slow, setSlow] = useState(false);
	const [progress, setProgress] = useState<ExportProgress | null>(null);
	const [said, setSaid] = useState("");
	const [asking, setAsking] = useState(false);
	// The keyboard reports a click with no count, and skips the motion.
	const [instant, setInstant] = useState(false);
	const drawn = useLingering(asking, DIALOG_FADE);
	const opener = useRef<HTMLButtonElement>(null);

	// Counting reads every scene, so it waits until the dialog is open.
	useEffect(() => {
		if (!asking) {
			return;
		}
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
	}, [root, changed, asking]);

	// Leaving while an export runs would hide how it ended.
	function leave() {
		if (!busy) {
			setAsking(false);
			opener.current?.focus();
		}
	}

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
		<>
			<button
				ref={opener}
				type="button"
				className="export__button"
				onClick={(event) => {
					setInstant(event.detail === 0);
					// Cleared here, so the last result stays put while it fades.
					setSaid("");
					setAsking(true);
				}}
			>
				Export the book…
			</button>

			{drawn &&
				createPortal(
					<Dialog
						label="Export"
						className="dialog export__dialog"
						closing={!asking}
						instant={instant}
						onLeave={leave}
					>
						<h2 className="dialog__title">Export</h2>
						<p className="dialog__lead">
							{extent === null ? "" : sized(extent)}
						</p>

						<div role="group" aria-label="Formats">
							{shown.map(({ key, name, what }) => (
								<button
									key={key}
									type="button"
									className="export__format"
									role="checkbox"
									aria-checked={formats.includes(key)}
									disabled={busy}
									onClick={() => {
										// What the last export said was
										// about the last choice.
										setSaid("");
										onFormats(flipped(formats, key));
									}}
								>
									<span className="export__tick" />
									<span className="export__label">
										<span>{name}</span>
										<span className="export__what">
											{what}
										</span>
									</span>
								</button>
							))}
						</div>

						{/* The bar and the line under it are always drawn, so
						    neither an export starting nor what it says at
						    the end moves anything. */}
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

						<div className="dialog__foot">
							<button
								type="button"
								className="quiet"
								disabled={busy}
								onClick={leave}
							>
								Close
							</button>
							<button
								type="button"
								className="export__run"
								disabled={busy || formats.length === 0}
								onClick={() => void run()}
							>
								{busy ? "Exporting…" : "Export…"}
							</button>
						</div>
					</Dialog>,
					document.body,
				)}
		</>
	);
}
