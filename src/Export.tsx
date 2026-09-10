import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { failure } from "./errors";
import { FORMATS, flipped, sized, wrote } from "./exporting";
import type { ExportFormat, Extent } from "./types";

type Props = {
	root: string;
	formats: ExportFormat[];
	onFormats: (formats: ExportFormat[]) => void;
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
	onBeforeExport,
}: Props) {
	const [extent, setExtent] = useState<Extent | null>(null);
	const [busy, setBusy] = useState(false);
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

		setBusy(true);
		try {
			await onBeforeExport();
			const files = await invoke<string[]>("export_book", {
				root,
				folder,
				formats,
			});
			setSaid(wrote(files, folder));
		} catch (error) {
			setSaid(failure(error).message);
		} finally {
			setBusy(false);
		}
	}

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
						onClick={() => onFormats(flipped(formats, key))}
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

			{/* Always drawn, so the line an export leaves moves nothing. */}
			<p className="export__said" role="status">
				{said}
			</p>
		</div>
	);
}
