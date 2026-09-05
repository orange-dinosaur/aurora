// A stopwatch for the reads that open the whole project. Aurora asks the
// backend for every document in several places, and how much that costs on a
// real-sized project has never been measured. Wrapping those calls here puts a
// number next to each one in the dev console. The shipped build drops all of
// it: `import.meta.env.DEV` is a constant Vite replaces, so the branch below
// folds away and the timing never runs.

/** Logs how long `work` took, under `name`, and passes it through untouched. */
export function timed<T>(name: string, work: Promise<T>): Promise<T> {
	if (!import.meta.env.DEV) {
		return work;
	}

	const start = performance.now();

	return work.finally(() => {
		const ms = performance.now() - start;
		console.log(`[timing] ${name} ${ms.toFixed(1)} ms`);
	});
}
