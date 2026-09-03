import { Chips, TextBox } from "./Field";
import type { FieldsHandle } from "./fields";
import { list, text } from "./frontmatter";

// The Info tab: what this document answers to, what it is filed under, and what
// the writer wants to remember about it. Custom fields join it later.

type Props = FieldsHandle & {
	/** Whether this is a page about a person or a place, which has names. */
	subject: boolean;
};

export default function Info({ fields, setField, subject }: Props) {
	/** A list field is dropped rather than written empty. */
	function setList(key: string, values: string[]) {
		setField(key, values.length === 0 ? null : values);
	}

	return (
		<>
			{subject && (
				<Chips
					label="Names"
					hint="The other names this answers to. Its title always counts."
					placeholder="Add a name"
					values={list(fields, "names")}
					onChange={(names) => setList("names", names)}
				/>
			)}

			<Chips
				label="Tags"
				placeholder="Add a tag"
				values={list(fields, "tags")}
				onChange={(tags) => setList("tags", tags)}
			/>

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
