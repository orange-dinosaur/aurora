import { counted } from "./cards";
import type { ExportFormat, ExportProgress, Extent, Format } from "./types";

type Offer = { key: ExportFormat; name: string; what: string };

/** A screenplay's formats. Rust refuses these for any other project. */
const SCRIPT: ExportFormat[] = ["fountain", "pdf", "fdx"];

/**
 * What the Export dialog offers a project of this format, in the order it
 * lists them: a screenplay's formats for a screenplay, a book's for the rest.
 */
export function offered(format: Format): Offer[] {
	const script = format === "screenplay";
	return FORMATS.filter((offer) => SCRIPT.includes(offer.key) === script);
}

/** Every format there is, in the order the dialog lists them. */
export const FORMATS: Offer[] = [
	{
		key: "markdown",
		name: "Markdown",
		what: "Plain files with light formatting",
	},
	{ key: "epub", name: "EPUB", what: "For most e-readers and bookshops" },
	{ key: "kepub", name: "KEPUB", what: "EPUB made for Kobo readers" },
	{ key: "docx", name: "DOCX", what: "For Word and for editors" },
	{ key: "text", name: "Plain text", what: "Just the words" },
	{
		key: "fountain",
		name: "Fountain",
		what: "Plain text any script app opens",
	},
	{ key: "pdf", name: "PDF", what: "Laid out on the page, ready to send" },
	{
		key: "fdx",
		name: "Final Draft",
		what: "For Final Draft and productions",
	},
];

/**
 * The ticks with one of them flipped. They stay in the order the panel lists
 * them, so ticking and unticking does not reshuffle what the manifest holds.
 */
export function flipped(
	ticked: ExportFormat[],
	key: ExportFormat,
): ExportFormat[] {
	const on = ticked.includes(key);

	return FORMATS.map((format) => format.key).filter((each) =>
		each === key ? !on : ticked.includes(each),
	);
}

/** What the export is about to take, said beside the button. */
export function sized(extent: Extent): string {
	const scenes =
		extent.scenes === 1
			? "1 scene"
			: `${extent.scenes.toLocaleString()} scenes`;

	return `${scenes} · ${counted(extent.words, null)}`;
}

/** What the line under the bar says while an export runs. */
export function doing(progress: ExportProgress): string {
	if (progress.stage === "writing") {
		return `Writing ${progress.name}`;
	}

	const { done, scenes } = progress;
	return `Reading scenes, ${done.toLocaleString()} of ${scenes.toLocaleString()}`;
}

/** How full the bar is, from 0 to 1. */
export function fraction(progress: ExportProgress): number {
	return progress.total === 0 ? 0 : progress.done / progress.total;
}

/** What an export wrote, and where. */
export function wrote(files: string[], folder: string): string {
	return `Wrote ${files.join(", ")} to ${folder}.`;
}

/** What the panel warns about before an export, if anything. */
export function warned(formats: ExportFormat[], hasCover: boolean): string {
	const epub = formats.includes("epub") || formats.includes("kepub");
	return epub && !hasCover
		? "No cover: most libraries will show a blank tile."
		: "";
}
