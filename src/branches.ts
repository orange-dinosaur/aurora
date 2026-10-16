// What the version menu asks before deleting a version whose checkpoints no
// other version holds.

export function unmergedQuestion(name: string, unmerged: number): string {
	if (unmerged === 1) {
		return `${name} has 1 checkpoint not merged into any other version. Delete it and the checkpoint?`;
	}
	return `${name} has ${unmerged.toLocaleString()} checkpoints not merged into any other version. Delete it and them?`;
}
