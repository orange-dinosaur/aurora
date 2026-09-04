import { AddField, Chips, TextBox } from "./Field";
import { useFieldNames, type FieldsHandle } from "./fields";
import { BUILT_IN, custom, list, text } from "./frontmatter";

// The Info tab: what this document answers to, what it is filed under, what the
// writer wants to remember about it, and any field they added themselves.

type Props = FieldsHandle & {
	/** Whether this is a page about a person or a place, which has names. */
	subject: boolean;
	root: string;
	/** Bumped whenever the project changes, so the suggestions are read again. */
	changed: number;
	onOpenTag: (tag: string) => void;
};

export default function Info({
	fields,
	setField,
	subject,
	root,
	changed,
	onOpenTag,
}: Props) {
	const { names, ask } = useFieldNames(root, changed);
	const own = custom(fields);

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
					onChange={(next) => setList("names", next)}
				/>
			)}

			{/* A tag goes somewhere: to the document it names, or to the list
			    of everything else wearing it. A name does not — it is a word
			    this page answers to, and the page is already open. */}
			<Chips
				label="Tags"
				placeholder="Add a tag"
				values={list(fields, "tags")}
				onChange={(next) => setList("tags", next)}
				onOpen={onOpenTag}
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

			{/* A field the writer added stays even while it is empty, unlike the
			    ones above: emptying it is not the same as not wanting it, and
			    the x is how it goes. */}
			{own.map((key) => (
				<TextBox
					key={key}
					label={key}
					value={text(fields, key)}
					placeholder=""
					rows={1}
					onChange={(value) => setField(key, value)}
					onRemove={() => setField(key, null)}
				/>
			))}

			<AddField
				names={names.filter((name) => !own.includes(name))}
				onAsk={ask}
				// A name Aurora already has a control for, or one this document
				// carries, is ignored: the field is on the page either way.
				onAdd={(name) => {
					if (!BUILT_IN.includes(name) && !fields.has(name)) {
						setField(name, "");
					}
				}}
			/>
		</>
	);
}
