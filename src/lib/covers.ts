const HUES = ["#f8a67c", "#f5bde6", "#8bd5ca", "#cba6f7", "#f9e2af", "#a6e3a1"];

/**
 * A cover drawn from a title, as a CSS background, for a book with no image.
 * The same title always gets the same one, so nothing needs saving.
 */
export function coverFor(title: string): string {
	let seed = 0;
	for (const letter of title) {
		seed = (seed * 31 + letter.charCodeAt(0)) >>> 0;
	}

	const pick = (shift: number) => HUES[(seed >>> shift) % HUES.length];

	return [
		`radial-gradient(at ${seed % 100}% ${(seed >>> 7) % 100}%, ${pick(2)}, transparent 62%)`,
		`radial-gradient(at ${(seed >>> 13) % 100}% ${(seed >>> 19) % 100}%, ${pick(9)}, transparent 62%)`,
		`linear-gradient(${seed % 360}deg, ${pick(5)}, ${pick(14)})`,
	].join(", ");
}
