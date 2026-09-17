/**
 * Every screen that is sitting on text the writer has not saved yet. A view
 * adds itself while it is mounted, so that closing the window can write all of
 * them without knowing which one the writer was looking at.
 */
const holders = new Set<() => Promise<void>>();

/**
 * Hold a flush for as long as a view is mounted. The returned function lets it
 * go again, so an effect can hand it straight back as its cleanup.
 */
export function holdUnsaved(flush: () => Promise<void>): () => void {
	holders.add(flush);
	return () => {
		holders.delete(flush);
	};
}

/**
 * Write everything still held. Nothing reports a failure: this runs on the way
 * out, when there is no longer anywhere to report it.
 */
export async function flushAll(): Promise<void> {
	await Promise.allSettled([...holders].map((flush) => flush()));
}
