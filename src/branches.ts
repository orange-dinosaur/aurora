// What the version menu asks before deleting a version whose checkpoints no
// other version holds.

/** Why a control that would change the project is greyed while a merge waits. */
export const MERGE_HOLDS = "Finish or abort the merge first";

export function unmergedQuestion(name: string, unmerged: number): string {
	if (unmerged === 1) {
		return `${name} has 1 checkpoint not merged into any other version. Delete it and the checkpoint?`;
	}
	return `${name} has ${unmerged.toLocaleString()} checkpoints not merged into any other version. Delete it and them?`;
}
