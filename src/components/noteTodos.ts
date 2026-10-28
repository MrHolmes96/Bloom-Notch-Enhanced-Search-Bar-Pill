import { Extension } from "@tiptap/core";
import type { Node as PMNode } from "@tiptap/pm/model";
import { Plugin, PluginKey, type EditorState } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

/**
 * Bullets and checklist items of a note as to-dos.
 *
 * A to-do made from a note's item remembers the note and the item's words
 * (`NoteItem.from`); the note remembers which of its items have been made
 * into to-dos (`NoteItem.todoLinks`), so an item is never added twice, not
 * even after its to-do was deleted. Ticking the to-do off ticks the item in
 * the note (a checklist item) or strikes it through (a bullet), working on
 * the note's Markdown, so it works with the note closed.
 *
 * While the note is open, the editor follows each linked item as it is
 * edited (a decoration that moves with the text, `TodoLinks`), so changing
 * an item's words changes its to-do's words too, and ticking or striking it
 * in the note ticks the to-do.
 */

/** Plain words, spaces evened out (how items are compared). */
export const normWords = (text: string) => text.replace(/\s+/g, " ").trim();

/** A Markdown line's words without the signs that format them. */
export function plainWords(markdown: string): string {
	// Escaped signs ("\_") are words, not formatting: set aside first.
	let out = markdown.replace(/\\([\\`*_{}[\]()#+\-.!~|>])/g, (_, c: string) => `\u0000${c.charCodeAt(0)}\u0001`);
	for (let i = 0; i < 4; i++) {
		const next = out
			.replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
			.replace(/(\*\*|__|\+\+|~~|\*|_|`)(?=\S)(.*?\S)\1/g, "$2");
		if (next === out) break;
		out = next;
	}
	out = out.replace(/\u0000(\d+)\u0001/g, (_, code: string) => String.fromCharCode(Number(code)));
	return normWords(out);
}

const LIST_LINE = /^(\s*)([-*+]|\d+[.)])(\s+)(\[[ xX]\]\s+)?(.*)$/;

/**
 * The note's Markdown with the item whose words are `words` ticked (a
 * checklist item: "[x]") or struck through (a bullet: "~~…~~"), or put back.
 * Null when the note has no such item.
 */
export function setNoteItemDone(markdown: string, words: string, done: boolean): string | null {
	const target = normWords(words);
	if (!target) return null;
	const lines = markdown.split("\n");
	for (let i = 0; i < lines.length; i++) {
		const match = LIST_LINE.exec(lines[i]);
		if (!match) continue;
		const [, indent, marker, space, box, content] = match;
		if (plainWords(content) !== target) continue;
		if (box) {
			lines[i] = `${indent}${marker}${space}[${done ? "x" : " "}] ${content}`;
		} else {
			const whole = /^~~(.*)~~$/.exec(content.trim());
			const unstruck = whole && !whole[1].includes("~~") ? whole[1] : content;
			lines[i] = done
				? `${indent}${marker}${space}~~${content.replace(/~~(.*?)~~/g, "$1").trim()}~~`
				: `${indent}${marker}${space}${unstruck}`;
		}
		return lines.join("\n");
	}
	return null;
}

/** One bullet, numbered or checklist item of the editor's document. */
export interface ListEntry {
	pos: number;
	/** Its own words (not the items under it). */
	text: string;
	/** Ticked (a checklist item) or struck through all along (a bullet). */
	done: boolean;
}

const isItem = (node: PMNode) => node.type.name === "listItem" || node.type.name === "taskItem";

export function entryOf(node: PMNode, pos: number): ListEntry {
	const first = node.firstChild;
	const text = first && first.isTextblock ? first.textContent.replace(/\s+/g, " ").trim() : "";
	let done = false;
	if (node.type.name === "taskItem") {
		done = !!node.attrs.checked;
	} else if (first && first.isTextblock && text) {
		let all = true;
		first.forEach((child) => {
			if (child.isText && child.text?.trim() && !child.marks.some((m) => m.type.name === "strike")) {
				all = false;
			}
		});
		done = all;
	}
	return { pos, text, done };
}

/** Every list item of the document, in order. */
export function listEntries(doc: PMNode): ListEntry[] {
	const found: ListEntry[] = [];
	doc.descendants((node, pos) => {
		if (isItem(node)) found.push(entryOf(node, pos));
		return true;
	});
	return found;
}

/** The list items the selection touches. */
export function selectedEntries(state: EditorState): ListEntry[] {
	const { from, to, empty } = state.selection;
	if (empty) return [];
	const found: ListEntry[] = [];
	state.doc.nodesBetween(from, to, (node, pos) => {
		if (isItem(node)) found.push(entryOf(node, pos));
		return true;
	});
	return found;
}

interface LinkSpec {
	link: { words: string };
}

export const todoLinksKey = new PluginKey<DecorationSet>("bloomTodoLinks");

/** Starts following these items (by position), each as made into a to-do with these words. */
export interface LinkRequest {
	pos: number;
	words: string;
}

function decorate(doc: PMNode, requests: LinkRequest[]) {
	return requests.flatMap(({ pos, words }) => {
		const node = doc.nodeAt(pos);
		if (!node || !isItem(node)) return [];
		return [Decoration.node(pos, pos + node.nodeSize, {}, { link: { words } } satisfies LinkSpec)];
	});
}

/** The items the note has made into to-dos, as the editor follows them now. */
export function linkedItems(state: EditorState) {
	const set = todoLinksKey.getState(state);
	if (!set) return [];
	return set
		.find()
		.map((decoration) => {
			const node = state.doc.nodeAt(decoration.from);
			if (!node || !isItem(node)) return null;
			return {
				entry: entryOf(node, decoration.from),
				link: (decoration.spec as LinkSpec).link
			};
		})
		.filter((item): item is { entry: ListEntry; link: LinkSpec["link"] } => item !== null);
}

/**
 * Follows the note's linked items while it is open. `linked` gives the words
 * of the items the note made into to-dos; each is found again by its words
 * when the note opens.
 */
export const TodoLinks = Extension.create<{ linked: () => string[] }>({
	name: "bloomTodoLinks",
	addOptions() {
		return { linked: () => [] };
	},
	addProseMirrorPlugins() {
		const linked = this.options.linked;
		return [
			new Plugin<DecorationSet>({
				key: todoLinksKey,
				state: {
					init: (_, state) => {
						const wanted = linked().map(normWords);
						const used = new Set<number>();
						const requests: LinkRequest[] = [];
						const entries = listEntries(state.doc);
						for (const words of wanted) {
							const entry = entries.find(
								(e) => !used.has(e.pos) && normWords(e.text) === words && words
							);
							if (!entry) continue;
							used.add(entry.pos);
							requests.push({ pos: entry.pos, words: entry.text });
						}
						return DecorationSet.create(state.doc, decorate(state.doc, requests));
					},
					apply: (tr, set) => {
						let next = set.map(tr.mapping, tr.doc);
						const requests = tr.getMeta(todoLinksKey) as LinkRequest[] | undefined;
						if (requests?.length) next = next.add(tr.doc, decorate(tr.doc, requests));
						return next;
					}
				},
				props: {
					decorations: (state) => todoLinksKey.getState(state)
				}
			})
		];
	}
});
