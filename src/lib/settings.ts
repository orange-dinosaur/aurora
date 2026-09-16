// The settings dialog's decisions that are not a click. Everything else in
// there is a button that writes a preference straight through, but a field the
// writer types a number into has to say what the typing meant.

/**
 * What a settings field holds, as the setting behind it. An empty field is
 * null: some settings take that as "none" and others fall back to their own
 * default, and which is which belongs with the setting rather than here.
 *
 * A number outside the range is pulled into it rather than refused. A field
 * that quietly ignores what was typed leaves the writer looking at a number
 * that is not the setting; one that clamps shows them what they got.
 */
export function bounded(
	typed: string,
	min: number,
	max: number,
): number | null {
	const digits = typed.replace(/\D/g, "");
	if (digits === "") {
		return null;
	}

	return Math.min(Math.max(Number(digits), min), max);
}
