/**
 * What a folder in the trash took in with it, for the line beside its name. A
 * folder that held nothing still went in as a folder and says so, or its row
 * would read as a document.
 */
export function trashed(inside: number) {
	if (inside === 0) {
		return "an empty folder";
	}
	return inside === 1
		? "a folder of 1 document"
		: `a folder of ${inside} documents`;
}
