import { counted } from "./cards";
import type { ExportFormat, ExportProgress, Extent } from "./types";

/** Every format the Export panel offers, in the order it lists them. */
export const FORMATS: { key: ExportFormat; name: string }[] = [
	{ key: "markdown", name: "Markdown" },
	{ key: "epub", name: "EPUB" },
	{ key: "kepub", name: "KEPUB" },
	{ key: "docx", name: "DOCX" },
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
