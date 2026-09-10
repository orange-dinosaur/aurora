import { counted } from "./cards";
import type { ExportFormat, Extent } from "./types";

/** Every format the Export panel offers, in the order it lists them. */
export const FORMATS: { key: ExportFormat; name: string }[] = [
	{ key: "markdown", name: "Markdown" },
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

/** What an export wrote, and where. */
export function wrote(files: string[], folder: string): string {
	return `Wrote ${files.join(", ")} to ${folder}.`;
}
