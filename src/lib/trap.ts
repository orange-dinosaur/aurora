// Keeps Tab inside a modal dialog, so focus never lands on what the scrim
// hides.

/**
 * Which of `count` controls Tab should move to, or null to let the browser
 * move as usual. `index` is the control that has focus, or -1 when focus is on
 * the dialog itself, as it is when the dialog opens.
 */
export function wrapTo(
	count: number,
	index: number,
	backwards: boolean,
): number | null {
	if (count === 0) {
		return null;
	}
	if (backwards) {
		return index <= 0 ? count - 1 : null;
	}
	return index === -1 || index === count - 1 ? 0 : null;
}

const FOCUSABLE =
	"button:not(:disabled), input:not(:disabled), textarea:not(:disabled), a[href], [tabindex]:not([tabindex='-1'])";

/** For a dialog's keydown. Only the edges are handled; Tab between them is
 * left to the browser. */
export function trapTab(event: KeyboardEvent, dialog: HTMLElement | null) {
	if (event.key !== "Tab" || dialog === null) {
		return;
	}
	// A radio group is one stop, on its checked radio.
	const controls = [
		...dialog.querySelectorAll<HTMLElement>(FOCUSABLE),
	].filter(
		(control) =>
			!(control instanceof HTMLInputElement) ||
			control.type !== "radio" ||
			control.checked,
	);
	const target = wrapTo(
		controls.length,
		controls.indexOf(document.activeElement as HTMLElement),
		event.shiftKey,
	);
	if (target !== null) {
		event.preventDefault();
		controls[target].focus();
	}
}
