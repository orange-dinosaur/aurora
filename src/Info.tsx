import { useState } from "react";
import TextBox from "./Field";
import Icon from "./Icon";
import type { FieldsHandle } from "./fields";
import { list, text } from "./frontmatter";

// The Info tab: what this document is filed under, and what the writer wants to
// remember about it. Names and custom fields join it later.

export default function Info({ fields, setField }: FieldsHandle) {
	const tags = list(fields, "tags");
	const [draft, setDraft] = useState("");

	function add() {
		const tag = draft.trim();

		// A tag already there is not added twice, and neither is nothing.
		if (tag !== "" && !tags.includes(tag)) {
			setField("tags", [...tags, tag]);
		}
		setDraft("");
	}

	function remove(tag: string) {
		const left = tags.filter((each) => each !== tag);
		setField("tags", left.length === 0 ? null : left);
	}

	return (
		<>
			<div className="field">
				<span className="field__label">Tags</span>
				{tags.length > 0 && (
					<ul className="chips">
						{tags.map((tag) => (
							<li key={tag} className="chip">
								{tag}
								<button
									type="button"
									className="chip__remove"
									aria-label={`Remove the tag ${tag}`}
									onClick={() => remove(tag)}
								>
									<Icon name="x" />
								</button>
							</li>
						))}
					</ul>
				)}
				<input
					className="field__line"
					type="text"
					value={draft}
					placeholder="Add a tag"
					aria-label="Add a tag"
					onChange={(event) => setDraft(event.target.value)}
					onKeyDown={(event) => {
						if (event.key === "Enter") {
							event.preventDefault();
							add();
						} else if (event.key === "Escape") {
							setDraft("");
						} else if (
							event.key === "Backspace" &&
							draft === "" &&
							tags.length > 0
						) {
							remove(tags[tags.length - 1]);
						}
					}}
					// Clicking away abandons what was half typed, the way every
					// other field that appears in place behaves.
					onBlur={() => setDraft("")}
				/>
			</div>

			<TextBox
				label="Remarks"
				value={text(fields, "remarks")}
				placeholder="Notes to yourself."
				rows={6}
				onChange={(value) =>
					setField("remarks", value === "" ? null : value)
				}
			/>
		</>
	);
}
