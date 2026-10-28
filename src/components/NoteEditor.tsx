import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { EditorContent, useEditor, useEditorState, type Editor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import { Placeholder } from "@tiptap/extensions";
import { Markdown } from "@tiptap/markdown";
import {
	Bold,
	ChevronLeft,
	Code,
	Download,
	Eraser,
	Italic,
	Link2,
	List,
	ListChecks,
	ListOrdered,
	ListPlus,
	ListTodo,
	Minus,
	Quote,
	Redo2,
	Strikethrough,
	Trash2,
	Underline,
	Undo2
} from "lucide-react";
import type { NoteItem, TodoSync } from "./NotesPanel";
import {
	linkedItems,
	listEntries,
	selectedEntries,
	TodoLinks,
	todoLinksKey,
	type ListEntry
} from "./noteTodos";
import { noteTitle, plainTextDoc, toMarkdownFile, toTextFile, type DocNode } from "./noteFormat";

/**
 * A note as a full page in the notch, with a formatting toolbar like the
 * Windows text editor's: text style (body, headings), bold, italic,
 * underline, strikethrough, bulleted, numbered and check lists, links, code,
 * quotes and dividers, clearing formatting, undo and redo. The note keeps its
 * formatting in Bloom; "Save as" writes it out as a .md or a .txt file, each
 * in the form that kind of file can hold (see noteFormat.ts).
 */

/** "17:42", or "26 Oct 17:42" on another day (24-hour clock). */
function whenText(time: number) {
	const date = new Date(time);
	const clock = date.toLocaleTimeString(undefined, {
		hour: "2-digit",
		minute: "2-digit",
		hour12: false
	});
	if (date.toDateString() === new Date().toDateString()) return clock;
	return `${date.toLocaleDateString(undefined, { day: "numeric", month: "short" })} ${clock}`;
}

interface NoteEditorProps {
	note: NoteItem;
	onPinChange: (pinned: boolean) => void;
	/** Saves the note's text (Markdown), a moment after each pause in typing,
	 * with what changed about its items that are to-dos. */
	onSave: (text: string, sync: TodoSync | null) => void;
	/** The note's own title (empty: its first line shows in the list). */
	onTitle: (title: string) => void;
	/** Turns adding the note's bullets to To-Do as they are written on or off. */
	onAutoTodo: (on: boolean) => void;
	/** Back to the list, with the text as it is now. */
	onClose: (text: string) => void;
	onDelete: () => void;
}

/** A toolbar button that leaves the text's selection where it is. */
function ToolButton({
	title,
	active,
	disabled,
	onRun,
	children
}: {
	title: string;
	active?: boolean;
	disabled?: boolean;
	onRun: () => void;
	children: React.ReactNode;
}) {
	return (
		<button
			type="button"
			className={`note-tool${active ? " active" : ""}`}
			title={title}
			aria-label={title}
			aria-pressed={active}
			disabled={disabled}
			onMouseDown={(e) => e.preventDefault()}
			onClick={onRun}
		>
			{children}
		</button>
	);
}

/** The editor is there to read: not let go of, and on the page. */
function alive(e: Editor | null | undefined): e is Editor {
	return !!e && !!e.schema && !e.isDestroyed;
}

function textStyle(editor: Editor) {
	for (const level of [1, 2, 3] as const) {
		if (editor.isActive("heading", { level })) return `h${level}`;
	}
	return "body";
}

/** The selected items that can still be made into to-dos, for the pop-up. */
function pickable(e: Editor) {
	const picked = selectedEntries(e.state).filter((entry) => entry.text);
	if (!picked.length) return null;
	const linked = new Set(linkedItems(e.state).map((l) => l.entry.pos));
	const count = picked.filter((entry) => !linked.has(entry.pos)).length;
	return { first: picked[0].pos, last: picked[picked.length - 1].pos, count };
}

export default function NoteEditor({
	note,
	onPinChange,
	onSave,
	onTitle,
	onAutoTodo,
	onClose,
	onDelete
}: NoteEditorProps) {
	const textRef = useRef<HTMLDivElement>(null);
	const [title, setTitle] = useState(note.title ?? "");
	/** Where the "Add to To-Do" pop-up sits over the selected items. */
	const [pickAt, setPickAt] = useState<{ top: number; left: number } | null>(null);
	const autoTodoRef = useRef(!!note.autoTodo);
	autoTodoRef.current = !!note.autoTodo;
	const panelRef = useRef<HTMLDivElement>(null);
	const saveTimer = useRef<number | null>(null);
	const closeRef = useRef<() => void>(() => {});
	// Kept open while a file is being saved (its dialog takes the focus).
	const savingFile = useRef(false);
	const [linkOpen, setLinkOpen] = useState(false);
	const [linkText, setLinkText] = useState("");
	const [saveMenu, setSaveMenu] = useState(false);
	const [status, setStatus] = useState<string | null>(null);

	// What the editor is made from is fixed for its life: the notch re-renders
	// many times a second (the music visualizer, the clock), and options that
	// look new on a render make the editor apply them again each time.
	const openLinkRef = useRef<() => void>(() => {});
	const [options] = useState(() => ({
		extensions: [
			StarterKit.configure({ link: { openOnClick: false, autolink: true } }),
			TaskList,
			TaskItem.configure({ nested: true }),
			Placeholder.configure({ placeholder: "Write your note…" }),
			Markdown,
			TodoLinks.configure({ linked: () => note.todoLinks ?? [] })
		],
		// Notes written before the toolbar are plain text, a paragraph a line.
		content: note.md ? note.text : (plainTextDoc(note.text) as never),
		contentType: (note.md ? "markdown" : "json") as "markdown" | "json",
		autofocus: "end" as const,
		editorProps: {
			attributes: { class: "note-doc", spellcheck: "false" },
			handleKeyDown: (_view: unknown, event: KeyboardEvent) => {
				if (event.key === "Escape") {
					closeRef.current();
					return true;
				}
				if (event.key.toLowerCase() === "k" && (event.ctrlKey || event.metaKey)) {
					openLinkRef.current();
					return true;
				}
				return false;
			}
		}
	}));

	const editor = useEditor({
		...options,
		onUpdate: () => {
			if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
			saveTimer.current = window.setTimeout(saveNow, 500);
		}
	});

	const state = useEditorState({
		editor,
		selector: ({ editor: e }) => {
			// An editor that has been let go of (the note closing, or React
			// starting the page over) has no schema left to read: nothing to show.
			if (!e || !e.schema) return null;
			try {
				return {
					style: textStyle(e),
					bold: e.isActive("bold"),
					italic: e.isActive("italic"),
					underline: e.isActive("underline"),
					strike: e.isActive("strike"),
					bullet: e.isActive("bulletList"),
					ordered: e.isActive("orderedList"),
					task: e.isActive("taskList"),
					link: e.isActive("link"),
					code: e.isActive("code"),
					quote: e.isActive("blockquote"),
					canUndo: e.can().undo(),
					canRedo: e.can().redo(),
					characters: e.state.doc.textContent.length,
					picked: pickable(e)
				};
			} catch {
				return null;
			}
		}
	});

	// The editor's own callbacks are the ones it was made with, so they reach
	// the editor and the save through these, never an earlier render's.
	const editorRef = useRef<Editor | null>(null);
	editorRef.current = editor;
	const onSaveRef = useRef(onSave);
	onSaveRef.current = onSave;

	const markdown = () => (alive(editor) ? editor.getMarkdown() : note.text);

	/**
	 * What changed about the note's to-dos: linked items whose words changed,
	 * whether each is ticked, and items to add (`extra`, and with auto-add on
	 * every item not added yet, except the one still being typed unless `all`).
	 */
	function todoSync(e: Editor, extra: ListEntry[], all: boolean): TodoSync {
		const linked = linkedItems(e.state);
		const renamed: TodoSync["renamed"] = [];
		for (const { entry, link } of linked) {
			if (entry.text && entry.text !== link.words) {
				renamed.push({ from: link.words, to: entry.text });
				link.words = entry.text;
			}
		}
		const taken = new Set(linked.map((l) => l.entry.pos));
		const fresh = new Map<number, ListEntry>();
		for (const entry of extra) {
			if (entry.text && !taken.has(entry.pos)) fresh.set(entry.pos, entry);
		}
		if (autoTodoRef.current) {
			const head = e.state.selection.$head.pos;
			for (const entry of listEntries(e.state.doc)) {
				if (!entry.text || taken.has(entry.pos)) continue;
				const node = e.state.doc.nodeAt(entry.pos);
				const first = node?.firstChild;
				// The item being typed waits until it is left (or the note closes).
				const typing =
					!all && !!first && head > entry.pos && head <= entry.pos + 1 + first.nodeSize;
				if (typing) continue;
				fresh.set(entry.pos, entry);
			}
		}
		const add = [...fresh.values()];
		if (add.length) {
			e.view.dispatch(
				e.state.tr
					.setMeta(todoLinksKey, add.map((a) => ({ pos: a.pos, words: a.text })))
					.setMeta("addToHistory", false)
			);
		}
		const now = linkedItems(e.state);
		return {
			add: add.map((a) => ({ text: a.text, done: a.done })),
			renamed,
			states: now.map(({ entry }) => ({ text: entry.text, done: entry.done })),
			links: now.map(({ entry, link }) => entry.text || link.words)
		};
	}

	function saveNow(extra: ListEntry[] = [], all = false) {
		if (saveTimer.current !== null) {
			window.clearTimeout(saveTimer.current);
			saveTimer.current = null;
		}
		const current = editorRef.current;
		if (!alive(current)) return;
		let sync: TodoSync | null = null;
		try {
			sync = todoSync(current, extra, all);
		} catch {
			sync = null;
		}
		onSaveRef.current(current.getMarkdown(), sync);
	}

	const close = () => {
		saveNow([], true);
		if (!alive(editor)) {
			onClose(note.text);
			return;
		}
		// A note with no words in it is passed on empty (and isn't kept).
		onClose(editor.getText().trim() ? markdown() : "");
	};
	closeRef.current = close;
	openLinkRef.current = openLink;

	// Leaving the page (or the notch closing) saves it.
	useEffect(() => {
		return () => {
			saveNow();
			onPinChange(false);
		};
	}, [editor]);

	function openLink() {
		if (!alive(editor)) return;
		setLinkText((editor.getAttributes("link").href as string | undefined) ?? "");
		setLinkOpen(true);
	}

	// The "Add to To-Do" pop-up sits just under the last selected item, so it
	// covers none of the selected text.
	const pickedKey = state?.picked
		? `${state.picked.first}:${state.picked.last}:${state.picked.count}`
		: "";
	useEffect(() => {
		if (!state?.picked || !alive(editor) || !textRef.current) {
			setPickAt(null);
			return;
		}
		const dom = editor.view.nodeDOM(state.picked.last);
		if (!(dom instanceof HTMLElement)) {
			setPickAt(null);
			return;
		}
		// Its own line only (an item's list under it isn't selected).
		const line = (dom.querySelector(":scope > p, :scope > div > p") as HTMLElement | null) ?? dom;
		let top = line.offsetHeight;
		let left = 0;
		let node: HTMLElement | null = line;
		while (node && node !== textRef.current) {
			top += node.offsetTop;
			left += node.offsetLeft;
			node = node.offsetParent as HTMLElement | null;
		}
		setPickAt({ top: top + 4, left: Math.max(0, left - 4) });
	}, [pickedKey, editor]);

	const addPicked = () => {
		if (!alive(editor)) return;
		const picked = selectedEntries(editor.state).filter((entry) => entry.text);
		saveNow(picked, true);
		setPickAt(null);
		// The selection goes, so the pop-up does.
		editor.chain().focus().setTextSelection(editor.state.selection.to).run();
	};

	const toggleAutoTodo = () => {
		const on = !note.autoTodo;
		autoTodoRef.current = on;
		onAutoTodo(on);
		// Turned on: the bullets already written go to To-Do as well.
		if (on) saveNow([], true);
	};

	const applyLink = () => {
		if (!alive(editor)) return;
		const href = linkText.trim();
		const chain = editor.chain().focus().extendMarkRange("link");
		if (!href) chain.unsetLink().run();
		else
			chain
				.setLink({ href: /^[a-z][a-z0-9+.-]*:/i.test(href) ? href : `https://${href}` })
				.run();
		setLinkOpen(false);
	};

	const saveAs = async (kind: "md" | "txt") => {
		setSaveMenu(false);
		if (!alive(editor)) return;
		const title =
			noteTitle(markdown(), true)
				.replace(/[\\/:*?"<>|]+/g, " ")
				.trim()
				.slice(0, 60) || "Note";
		const contents =
			kind === "md" ? toMarkdownFile(markdown()) : toTextFile(editor.getJSON() as DocNode);
		savingFile.current = true;
		onPinChange(true);
		try {
			const { save } = await import("@tauri-apps/plugin-dialog");
			const path = await save({
				title: kind === "md" ? "Save note as Markdown" : "Save note as text",
				defaultPath: `${title}.${kind}`,
				filters: [
					kind === "md"
						? { name: "Markdown", extensions: ["md"] }
						: { name: "Text", extensions: ["txt"] }
				]
			});
			if (path) {
				await invoke("save_note_file", { path, contents });
				setStatus(`Saved ${path.split(/[\\/]/).pop()}`);
				window.setTimeout(() => setStatus(null), 4000);
			}
		} catch (e) {
			setStatus(`Couldn't save: ${String(e)}`);
		} finally {
			savingFile.current = false;
			if (!panelRef.current?.contains(document.activeElement)) onPinChange(false);
			if (alive(editor)) editor.commands.focus();
		}
	};

	const run = (action: (e: Editor) => void) => () => {
		if (alive(editor)) action(editor);
	};

	return (
		<div
			ref={panelRef}
			className="notes-panel notes-editor"
			onClick={(e) => e.stopPropagation()}
			onKeyDown={(e) => e.stopPropagation()}
			// Typing keeps the notch open; clicking another app lets it close.
			onFocus={() => onPinChange(true)}
			onBlur={(e) => {
				if (panelRef.current?.contains(e.relatedTarget as Node | null)) return;
				saveNow();
				if (!savingFile.current) onPinChange(false);
			}}
		>
			<div className="notes-editor-bar">
				<button type="button" className="notes-back" onClick={close} title="Back to the list (Esc)">
					<ChevronLeft size={14} />
					Notes
				</button>
				<input
					className="note-title-input"
					value={title}
					placeholder={noteTitle(note.text, !!note.md) || "Title"}
					title="The note's title in the list (empty: its first line)"
					spellCheck={false}
					onChange={(e) => {
						setTitle(e.target.value);
						onTitle(e.target.value);
					}}
					onKeyDown={(e) => {
						e.stopPropagation();
						if (e.key === "Enter" || e.key === "Escape") {
							e.preventDefault();
							if (alive(editor)) editor.commands.focus();
						}
					}}
				/>
				<span className="notes-editor-meta">
					{status ??
						`${note.updated ? `Edited ${whenText(note.updated)}` : `Created ${whenText(note.created)}`} · ${
							state?.characters ?? 0
						} ${state?.characters === 1 ? "character" : "characters"}`}
				</span>
				<div className="note-save">
					<button
						type="button"
						className="notes-back"
						title="Save the note as a file"
						onMouseDown={(e) => e.preventDefault()}
						onClick={() => setSaveMenu((open) => !open)}
					>
						<Download size={13} />
						Save as
					</button>
					{saveMenu && (
						<div className="note-save-menu" role="menu">
							<button
								type="button"
								role="menuitem"
								onMouseDown={(e) => e.preventDefault()}
								onClick={() => void saveAs("md")}
							>
								Markdown (.md)
								<span>Keeps all formatting</span>
							</button>
							<button
								type="button"
								role="menuitem"
								onMouseDown={(e) => e.preventDefault()}
								onClick={() => void saveAs("txt")}
							>
								Text (.txt)
								<span>Laid out as plain text</span>
							</button>
						</div>
					)}
				</div>
				<button
					type="button"
					className="notes-delete visible"
					title="Delete note"
					onClick={onDelete}
				>
					<Trash2 size={12} />
				</button>
			</div>

			<div className="note-toolbar" role="toolbar" aria-label="Formatting">
				<select
					className="note-style"
					title="Text style"
					value={state?.style ?? "body"}
					onChange={(e) => {
						if (!editor) return;
						const value = e.target.value;
						const chain = editor.chain().focus();
						if (value === "body") chain.setParagraph().run();
						else chain.setHeading({ level: Number(value.slice(1)) as 1 | 2 | 3 }).run();
					}}
				>
					<option value="body">Body</option>
					<option value="h1">Heading 1</option>
					<option value="h2">Heading 2</option>
					<option value="h3">Heading 3</option>
				</select>
				<span className="note-tool-gap" />
				<ToolButton title="Bold (Ctrl+B)" active={state?.bold} onRun={run((e) => e.chain().focus().toggleBold().run())}>
					<Bold size={14} />
				</ToolButton>
				<ToolButton title="Italic (Ctrl+I)" active={state?.italic} onRun={run((e) => e.chain().focus().toggleItalic().run())}>
					<Italic size={14} />
				</ToolButton>
				<ToolButton
					title="Underline (Ctrl+U)"
					active={state?.underline}
					onRun={run((e) => e.chain().focus().toggleUnderline().run())}
				>
					<Underline size={14} />
				</ToolButton>
				<ToolButton
					title="Strikethrough (Ctrl+Shift+S)"
					active={state?.strike}
					onRun={run((e) => e.chain().focus().toggleStrike().run())}
				>
					<Strikethrough size={14} />
				</ToolButton>
				<span className="note-tool-gap" />
				<ToolButton
					title="Bulleted list"
					active={state?.bullet}
					onRun={run((e) => e.chain().focus().toggleBulletList().run())}
				>
					<List size={14} />
				</ToolButton>
				<ToolButton
					title="Numbered list"
					active={state?.ordered}
					onRun={run((e) => e.chain().focus().toggleOrderedList().run())}
				>
					<ListOrdered size={14} />
				</ToolButton>
				<ToolButton
					title="Checklist"
					active={state?.task}
					onRun={run((e) => e.chain().focus().toggleTaskList().run())}
				>
					<ListChecks size={14} />
				</ToolButton>
				<button
					type="button"
					className={`note-tool note-auto-todo${note.autoTodo ? " active" : ""}`}
					title={
						note.autoTodo
							? "Bullets and checklist items go to To-Do as you write them. Click to turn off"
							: "Add this note's bullets and checklist items to To-Do as you write them. Click to turn on"
					}
					aria-pressed={!!note.autoTodo}
					onMouseDown={(e) => e.preventDefault()}
					onClick={toggleAutoTodo}
				>
					<ListPlus size={14} />
					Auto To-Do {note.autoTodo ? "On" : "Off"}
				</button>
				<span className="note-tool-gap" />
				<ToolButton title="Link (Ctrl+K)" active={state?.link} onRun={openLink}>
					<Link2 size={14} />
				</ToolButton>
				<ToolButton title="Code" active={state?.code} onRun={run((e) => e.chain().focus().toggleCode().run())}>
					<Code size={14} />
				</ToolButton>
				<ToolButton
					title="Quote"
					active={state?.quote}
					onRun={run((e) => e.chain().focus().toggleBlockquote().run())}
				>
					<Quote size={14} />
				</ToolButton>
				<ToolButton
					title="Divider"
					onRun={run((e) => e.chain().focus().setHorizontalRule().run())}
				>
					<Minus size={14} />
				</ToolButton>
				<span className="note-tool-gap" />
				<ToolButton
					title="Clear formatting"
					onRun={run((e) => e.chain().focus().unsetAllMarks().clearNodes().run())}
				>
					<Eraser size={14} />
				</ToolButton>
				<span className="note-tool-spacer" />
				<ToolButton
					title="Undo (Ctrl+Z)"
					disabled={!state?.canUndo}
					onRun={run((e) => e.chain().focus().undo().run())}
				>
					<Undo2 size={14} />
				</ToolButton>
				<ToolButton
					title="Redo (Ctrl+Y)"
					disabled={!state?.canRedo}
					onRun={run((e) => e.chain().focus().redo().run())}
				>
					<Redo2 size={14} />
				</ToolButton>
			</div>

			{linkOpen && (
				<div className="note-link-row">
					<Link2 size={13} />
					<input
						className="notes-input"
						autoFocus
						placeholder="Web address (empty removes the link)"
						value={linkText}
						spellCheck={false}
						onChange={(e) => setLinkText(e.target.value)}
						onKeyDown={(e) => {
							e.stopPropagation();
							if (e.key === "Enter") applyLink();
							if (e.key === "Escape") {
								setLinkOpen(false);
								if (alive(editor)) editor.commands.focus();
							}
						}}
					/>
					<button type="button" className="notes-back" onClick={applyLink}>
						Apply
					</button>
				</div>
			)}

			<div
				ref={textRef}
				className="notes-editor-text"
				onWheel={(e) => e.stopPropagation()}
			>
				<EditorContent editor={editor} />
				{pickAt && state?.picked && (
					<div className="note-todo-pop" style={{ top: pickAt.top, left: pickAt.left }}>
						{state.picked.count > 0 ? (
							<button
								type="button"
								onMouseDown={(e) => e.preventDefault()}
								onClick={addPicked}
							>
								<ListTodo size={13} />
								{state.picked.count === 1 ? "Add to To-Do" : `Add ${state.picked.count} to To-Do`}
							</button>
						) : (
							<span>
								<ListTodo size={13} />
								Already in To-Do
							</span>
						)}
					</div>
				)}
			</div>
		</div>
	);
}
