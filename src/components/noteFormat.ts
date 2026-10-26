/**
 * A note's formatting kept as the note's own text (Markdown, the way the
 * notch's editor writes it), and turned into what each kind of file can
 * hold when it is saved:
 *
 * - .md: Markdown, with underline written as <u>…</u> (Markdown has no sign
 *   for it, and <u> shows underlined in every Markdown app).
 * - .txt: plain text laid out to read as it looks: headings underlined with
 *   ═ and ─, lists with • and their numbers, checklists with ☐ and ☑, links
 *   as "text (address)", quotes with ▏, code indented, dividers as a line.
 *   Bold, italic, underline and strikethrough can't be stored in plain text,
 *   so their words are kept as they are.
 */

/** A node of the editor's document (ProseMirror JSON). */
export interface DocNode {
	type: string;
	attrs?: Record<string, unknown>;
	content?: DocNode[];
	text?: string;
	marks?: { type: string; attrs?: Record<string, unknown> }[];
}

/** The note's Markdown as a .md file. */
export function toMarkdownFile(markdown: string): string {
	// "++text++" is the editor's underline; Markdown apps know <u>.
	return `${markdown.replace(/\+\+(?=\S)([^\n]*?\S)\+\+/g, "<u>$1</u>").trimEnd()}\n`;
}

/** Inline text of a node: links as "text (address)", line breaks kept. */
function inlineText(node: DocNode): string {
	if (node.type === "text") {
		const text = node.text ?? "";
		const link = node.marks?.find((m) => m.type === "link");
		const href = typeof link?.attrs?.href === "string" ? (link.attrs.href as string) : "";
		return href && href !== text ? `${text} (${href})` : text;
	}
	if (node.type === "hardBreak") return "\n";
	return (node.content ?? []).map(inlineText).join("");
}

const indent = (text: string, prefix: string) =>
	text
		.split("\n")
		.map((line) => (line ? prefix + line : line))
		.join("\n");

/** One list item: its marker, then its text, with what is under it indented. */
function listItem(node: DocNode, marker: string, depth: number): string {
	const pad = "   ".repeat(depth);
	const parts = (node.content ?? []).map((child) =>
		child.type === "paragraph" ? inlineText(child) : block(child, depth + 1)
	);
	const [first = "", ...rest] = parts;
	const firstLines = first.split("\n");
	const head = [
		`${pad}${marker}${firstLines[0]}`,
		...firstLines.slice(1).map((line) => `${pad}${" ".repeat(marker.length)}${line}`)
	].join("\n");
	return [head, ...rest].join("\n");
}

function block(node: DocNode, depth = 0): string {
	const children = node.content ?? [];
	switch (node.type) {
		case "heading": {
			const text = inlineText(node);
			const level = Number(node.attrs?.level ?? 1);
			const width = Math.max(...text.split("\n").map((line) => [...line].length), 1);
			if (level === 1) return `${text}\n${"═".repeat(width)}`;
			if (level === 2) return `${text}\n${"─".repeat(width)}`;
			return text;
		}
		case "paragraph":
			return inlineText(node);
		case "bulletList":
			return children.map((item) => listItem(item, "• ", depth)).join("\n");
		case "orderedList": {
			const start = Number(node.attrs?.start ?? 1);
			return children.map((item, i) => listItem(item, `${start + i}. `, depth)).join("\n");
		}
		case "taskList":
			return children
				.map((item) => listItem(item, item.attrs?.checked ? "☑ " : "☐ ", depth))
				.join("\n");
		case "blockquote":
			return indent(children.map((c) => block(c, depth)).join("\n\n"), "▏ ");
		case "codeBlock":
			return indent(inlineText(node), "    ");
		case "horizontalRule":
			return "──────────────────────────────";
		default:
			return children.length ? children.map((c) => block(c, depth)).join("\n\n") : inlineText(node);
	}
}

/** The note as a .txt file, laid out to read as it looks in the notch. */
export function toTextFile(doc: DocNode): string {
	const blocks = (doc.content ?? []).map((node) => block(node));
	// Trailing empty paragraphs aren't kept.
	while (blocks.length && !blocks[blocks.length - 1].trim()) blocks.pop();
	return `${blocks.join("\n\n").replace(/\r?\n/g, "\r\n")}\r\n`;
}

/**
 * A note's first line as the list shows it: its words without the signs
 * that format them.
 */
export function noteTitle(text: string, markdown: boolean): string {
	const line = text.split("\n").find((l) => l.trim())?.trim() ?? "";
	if (!markdown) return line;
	return line
		.replace(/^(#{1,6}\s+|>\s*|[-*+]\s+\[[ xX]\]\s+|[-*+]\s+|\d+[.)]\s+)/, "")
		.replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
		.replace(/(\*\*|__|\+\+|~~|\*|_|`)(?=\S)(.*?\S)\1/g, "$2")
		.replace(/\\([\\`*_{}[\]()#+\-.!~|>])/g, "$1")
		.trim();
}

/** Whether the note has more than its first line. */
export function noteHasMore(text: string): boolean {
	return text.trim().includes("\n");
}

/** A note from before the editor (plain text): one paragraph per line. */
export function plainTextDoc(text: string): DocNode {
	return {
		type: "doc",
		content: text.split("\n").map((line) =>
			line ? { type: "paragraph", content: [{ type: "text", text: line }] } : { type: "paragraph" }
		)
	};
}
