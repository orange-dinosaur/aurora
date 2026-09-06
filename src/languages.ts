// What language a book is in. EPUB's `dc:language` and DOCX both want a BCP 47
// tag rather than a name, so the tag is what is stored and the name is only
// ever what the picker draws.

export type Language = {
	/** A BCP 47 tag such as `en` or `pt-BR`. */
	tag: string;
	name: string;
};

/**
 * The languages Aurora offers. Not a complete list, and not meant to be: a tag
 * the writer's computer is set to is added to it when it is missing, which is
 * the case that would otherwise send them looking for a language that is not
 * there.
 */
const KNOWN: Language[] = [
	{ tag: "en", name: "English" },
	{ tag: "es", name: "Spanish" },
	{ tag: "fr", name: "French" },
	{ tag: "de", name: "German" },
	{ tag: "it", name: "Italian" },
	{ tag: "pt", name: "Portuguese" },
	{ tag: "pt-BR", name: "Portuguese (Brazil)" },
	{ tag: "nl", name: "Dutch" },
	{ tag: "sv", name: "Swedish" },
	{ tag: "da", name: "Danish" },
	{ tag: "nb", name: "Norwegian" },
	{ tag: "fi", name: "Finnish" },
	{ tag: "is", name: "Icelandic" },
	{ tag: "pl", name: "Polish" },
	{ tag: "cs", name: "Czech" },
	{ tag: "ru", name: "Russian" },
	{ tag: "uk", name: "Ukrainian" },
	{ tag: "el", name: "Greek" },
	{ tag: "tr", name: "Turkish" },
	{ tag: "ar", name: "Arabic" },
	{ tag: "he", name: "Hebrew" },
	{ tag: "hi", name: "Hindi" },
	{ tag: "zh", name: "Chinese" },
	{ tag: "ja", name: "Japanese" },
	{ tag: "ko", name: "Korean" },
];

/** What the browser calls a tag, or the tag itself where it has no name. */
function display(tag: string): string {
	try {
		return (
			new Intl.DisplayNames(["en"], { type: "language" }).of(tag) ?? tag
		);
	} catch {
		return tag;
	}
}

/**
 * The known language a tag names, matched on the whole tag first so `pt-BR` is
 * Brazilian rather than Portuguese, and then on the part before the dash so
 * `en-GB` is still English.
 */
function find(tag: string): Language | null {
	const wanted = tag.trim().toLowerCase();
	if (wanted === "") {
		return null;
	}

	const primary = wanted.split("-")[0];
	return (
		KNOWN.find((language) => language.tag.toLowerCase() === wanted) ??
		KNOWN.find((language) => language.tag.toLowerCase() === primary) ??
		null
	);
}

/**
 * The list to offer, with the language the computer is set to at the head. A
 * writer's own language is the answer nearly every time, and it should not have
 * to be hunted for down a list of twenty-five.
 */
export function offered(locale: string): Language[] {
	const first = find(locale);
	if (first !== null) {
		return [first, ...KNOWN.filter((language) => language !== first)];
	}

	const tag = locale.trim();
	return tag === "" ? KNOWN : [{ tag, name: display(tag) }, ...KNOWN];
}

/** What to write on the picker for what is stored. */
export function named(tag: string): string {
	const found = find(tag);
	if (found !== null) {
		return found.name;
	}

	return tag.trim() === "" ? "" : display(tag.trim());
}
