use super::*;

/// A document cut into its front matter block and the prose under it. The fence
/// is spelled the same as the one `FRONT_MATTER` matches in
/// `src/frontmatter.ts`, which is the end that reads and writes what is inside
/// the block; the two have to agree on where a document starts. The block comes
/// back with its fences and nothing after them, which is the shape the parser
/// there is given.
///
/// The block closes on the first later line that is `---` and nothing but
/// spaces or tabs, and only counts when every line between the fences reads
/// as front matter. A scene that opens with a scene break would otherwise
/// have its first stretch of prose lifted off as a block.
pub(super) fn split(text: &str) -> (&str, &str) {
	let none = ("", text);
	let Some(rest) = text.strip_prefix("---") else {
		return none;
	};
	let Some(rest) = rest
		.strip_prefix("\r\n")
		.or_else(|| rest.strip_prefix('\n'))
	else {
		return none;
	};

	let mut offset = text.len() - rest.len();
	for (at, line) in rest.split_inclusive('\n').enumerate() {
		let bare = line.trim_end_matches(['\r', '\n']);
		if at > 0
			&& let Some(tail) = bare.strip_prefix("---")
			&& tail.chars().all(|c| c == ' ' || c == '\t')
		{
			return (&text[..offset + 3], &text[offset + line.len()..]);
		}
		if (at == 0 && bare.trim().is_empty()) || !front_matter_line(bare) {
			return none;
		}
		offset += line.len();
	}
	none
}

/// Whether a line could sit inside a front matter block: blank, a comment,
/// indented, a list item, or a `key:` line. The same test as in
/// `src/frontmatter.ts`.
pub(super) fn front_matter_line(line: &str) -> bool {
	line.trim().is_empty()
		|| line.trim_start().starts_with('#')
		|| line.starts_with(char::is_whitespace)
		|| line
			.strip_prefix('-')
			.is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
		|| (line.starts_with(|c: char| c.is_alphanumeric() || c == '_') && line.contains(':'))
}

/// A document's prose, with any front matter block lifted off the top.
pub(crate) fn body(text: &str) -> &str {
	split(text).1
}

/// Whether a line is only dashes, stars or underscores: a scene break.
pub(super) fn scene_break(line: &str) -> bool {
	let mut marks = line.trim().chars().filter(|c| !c.is_whitespace());
	let Some(first) = marks.next().filter(|c| matches!(c, '-' | '*' | '_')) else {
		return false;
	};
	let mut count = 1;
	for mark in marks {
		if mark != first {
			return false;
		}
		count += 1;
	}
	count >= 3
}

/// A line without the quote marks, heading hashes, bullet or number it opens
/// with.
pub(super) fn without_opening(line: &str) -> &str {
	let mut rest = line.trim_start();
	while let Some(after) = rest.strip_prefix('>') {
		rest = after.trim_start();
	}

	let spaced = |after: &str| after.chars().next().is_none_or(char::is_whitespace);

	let hashes = rest.chars().take_while(|c| *c == '#').count();
	if (1..=6).contains(&hashes) && spaced(&rest[hashes..]) {
		return &rest[hashes..];
	}

	let mut chars = rest.chars();
	if let Some(first) = chars.next()
		&& matches!(first, '-' | '*' | '+')
		&& chars.next().is_some_and(char::is_whitespace)
	{
		return &rest[1..];
	}

	let digits = rest.chars().take_while(char::is_ascii_digit).count();
	if digits > 0
		&& rest[digits..].starts_with('.')
		&& rest.len() > digits + 1
		&& spaced(&rest[digits + 1..])
	{
		return &rest[digits + 1..];
	}

	rest
}

/// One line with its emphasis markers, backticks and escapes taken off.
pub(super) fn without_marks(line: &str) -> String {
	let mut out = String::with_capacity(line.len());
	let mut chars = line.chars().peekable();
	while let Some(mark) = chars.next() {
		match mark {
			'\\' if chars.peek().is_some_and(char::is_ascii_punctuation) => {
				out.extend(chars.next());
			}
			'~' if chars.peek() == Some(&'~') => {
				chars.next();
			}
			'*' | '_' | '`' => {}
			_ => out.push(mark),
		}
	}
	out
}

/// Markdown with its markers taken off, leaving the words a reader meets. The
/// editor's `plain` in `words.ts` does the same, and both run one table of cases.
pub(crate) fn plain(markdown: &str) -> String {
	markdown
		.split('\n')
		.filter(|line| !scene_break(line))
		.map(|line| without_marks(without_opening(line)))
		.collect::<Vec<_>>()
		.join("\n")
}

/// How many words a document's body holds, counted from what a reader sees.
pub(crate) fn words_of(markdown: &str) -> usize {
	plain(markdown).split_whitespace().count()
}

/// A document with its prose replaced and its front matter block as it was. A
/// block that closes at the very end of the file needs a newline to stay a block.
pub(crate) fn with_body(text: &str, prose: &str) -> String {
	let head = &text[..text.len() - body(text).len()];
	let close = if head.is_empty() || head.ends_with('\n') {
		""
	} else {
		"\n"
	};
	format!("{head}{close}{prose}")
}

/// The key a scene carries in its front matter, and a folder in its fields,
/// when the writer has taken it out of the book. It is written only when it is
/// `false`, so anything that says nothing is in: a project written before the
/// switch existed exports whole.
pub const IN_BOOK: &str = "inBook";

/// Whether a document is in the book, from its text. What a field is stays
/// defined in `src/frontmatter.ts`; this looks for the one key the compile
/// walk cannot ask the other end about, since it never opens these files.
pub fn in_book(text: &str) -> bool {
	let (front, _) = split(text);

	!front.lines().any(|line| {
		// Only at the left margin: a key of the same name indented under
		// another field belongs to that field, not to Aurora.
		line.strip_prefix(IN_BOOK)
			.and_then(|rest| rest.strip_prefix(':'))
			.is_some_and(denied)
	})
}

/// Whether a folder is in the book, from the fields the manifest holds for it.
/// A folder has no file of its own to keep front matter in, so its flag is a
/// field like the rest of them.
pub fn folder_in_book(fields: &Fields) -> bool {
	match fields.get(IN_BOOK) {
		Some(tree::Value::Flag(said)) => *said,
		// A writer who typed the field into the manifest themselves wrote it
		// as text, and meant it.
		Some(tree::Value::Text(said)) => !denied(said),
		_ => true,
	}
}

/// What YAML reads as no, whichever way it was spelled, and whether or not the
/// writer put quotes round it.
pub(super) fn denied(value: &str) -> bool {
	let value = value.trim();
	let value = value
		.strip_prefix(['"', '\''])
		.and_then(|rest| rest.strip_suffix(['"', '\'']))
		.unwrap_or(value);

	matches!(value.to_ascii_lowercase().as_str(), "false" | "no" | "off")
}

/// The opening of a document collapsed onto one line and cut to something a
/// card can hold.
pub(super) fn excerpt(text: &str) -> String {
	let mut excerpt = String::new();
	let mut length = 0;

	for word in text.split_whitespace() {
		let separator = usize::from(!excerpt.is_empty());
		let width = word.chars().count();

		if length + separator + width > EXCERPT_CHARS {
			// A first word longer than the whole excerpt still has to show
			// something.
			if excerpt.is_empty() {
				excerpt.extend(word.chars().take(EXCERPT_CHARS));
			}
			excerpt.push('\u{2026}');
			break;
		}

		if separator == 1 {
			excerpt.push(' ');
		}
		excerpt.push_str(word);
		length += separator + width;
	}

	excerpt
}
