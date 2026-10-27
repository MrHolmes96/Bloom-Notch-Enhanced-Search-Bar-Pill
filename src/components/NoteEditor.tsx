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
	Minus,
	Quote,
	Redo2,
	Strikethrough,
	Trash2,
	Underline,
	Undo2
} from "lucide-react";
import type { NoteItem } from "./NotesPanel";
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
	/** Saves the note's text (Markdown), a moment after each pause in typing. */
	onSave: (text: string) => void;
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

export default function NoteEditor({
	note,
	onPinChange,
	onSave,
	onClose,
	onDelete
}: NoteEditorProps) {
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
			Markdown
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
					characters: e.state.doc.textContent.length
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

	function saveNow() {
		if (saveTimer.current !== null) {
			window.clearTimeout(saveTimer.current);
			saveTimer.current = null;
		}
		const current = editorRef.current;
		if (alive(current)) onSaveRef.current(current.getMarkdown());
	}

	const close = () => {
		saveNow();
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

			<div className="notes-editor-text" onWheel={(e) => e.stopPropagation()}>
				<EditorContent editor={editor} />
			</div>
		</div>
	);
}
