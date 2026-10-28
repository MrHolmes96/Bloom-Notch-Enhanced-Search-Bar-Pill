import { lazy, memo, Suspense, useEffect, useMemo, useRef, useState } from "react";
import {
	ArrowDownUp,
	Check,
	ListTodo,
	Maximize2,
	Pencil,
	StickyNote,
	X
} from "lucide-react";
import { saveStoredSetting } from "../settings/useStoredSetting";
import { noteHasMore, noteTitle } from "./noteFormat";
import { setNoteItemDone } from "./noteTodos";
import { PanelBoundary } from "./PanelBoundary";
import "./NotesPanel.css";

// The editor (and its toolbar) is its own download, fetched when the notes
// page opens, so it is ready before a note is.
const loadNoteEditor = () => import("./NoteEditor");
const NoteEditor = lazy(loadNoteEditor);

/**
 * Quick notes and to-dos in the notch, for things that don't belong in a
 * calendar. They are shown in two sections, Notes and To-Do (or one of them),
 * sorted as chosen. To-dos can be ticked off and cleared; a note opens as a
 * full page in the notch on a double-click, for longer text, so no separate
 * notes app is needed.
 */

export interface NoteItem {
	id: string;
	text: string;
	todo: boolean;
	done: boolean;
	created: number;
	/** When the text was last changed (absent until it is). */
	updated?: number;
	/** The text is Markdown, as the note's editor writes it (notes from
	 * before the editor's toolbar are plain text). */
	md?: boolean;
	/** A note's own title for the list (else its first line shows). */
	title?: string;
	/** A note: its bullets and checklist items go to To-Do as they are written. */
	autoTodo?: boolean;
	/** A note: the words of its items made into to-dos (see noteTodos.ts). */
	todoLinks?: string[];
	/** A to-do made from a note's item: that note, and the item's words. */
	from?: { note: string; text: string };
}

/** What the note's editor hands over with its text (see noteTodos.ts). */
export interface TodoSync {
	/** Items to make into to-dos. */
	add: { text: string; done: boolean }[];
	/** Linked items whose words changed. */
	renamed: { from: string; to: string }[];
	/** Each linked item, ticked or struck through or not. */
	states: { text: string; done: boolean }[];
	/** The words of every linked item now. */
	links: string[];
}

/** How a note shows in the list: its title, else its first line. */
export function noteLabel(item: NoteItem): string {
	return item.title?.trim() || noteTitle(item.text, !!item.md);
}

export type NotesView = "all" | "notes" | "todos";
export type NotesSort = "newest" | "oldest" | "az" | "za";

/** What the notch needs to know to size itself around the panel. */
export interface NotesLayout {
	view: NotesView;
	/** A note is open as a full page. */
	editorOpen: boolean;
}

export const NOTES_KEY = "bloom-notch-notes";
const VIEW_KEY = "bloom-notch-notes-view";
const SORT_KEY = "bloom-notch-notes-sort";
export const NOTES_ROW_HEIGHT = 30;
/** A section's heading in the list. */
const HEADER_HEIGHT = 22;
/** The list scrolls past this height. */
const LIST_MAX_HEIGHT = 5 * NOTES_ROW_HEIGHT + 2 * HEADER_HEIGHT;
/**
 * While a note is open the notch takes this share of the screen, from its top
 * edge: 68 % of its width and 63 % of its height, on any screen and size.
 */
const EDITOR_WIDTH_SHARE = 0.68;
const EDITOR_HEIGHT_SHARE = 0.63;

/** The notch's size with a note open, in the notch's own units (`scale` is
 * its size setting, which zooms it). */
export function notesEditorSize(scale: number) {
	const zoom = scale > 0 ? scale : 1;
	return {
		width: Math.round((window.screen.width * EDITOR_WIDTH_SHARE) / zoom),
		height: Math.round((window.screen.height * EDITOR_HEIGHT_SHARE) / zoom)
	};
}

const SORT_LABELS: Record<NotesSort, string> = {
	newest: "Newest first",
	oldest: "Oldest first",
	az: "A to Z",
	za: "Z to A"
};
const SORT_ORDER: NotesSort[] = ["newest", "oldest", "az", "za"];

export function parseNotes(raw: unknown): NoteItem[] {
	if (Array.isArray(raw)) return raw as NoteItem[];
	if (typeof raw !== "string" || !raw) return [];
	try {
		const parsed = JSON.parse(raw);
		return Array.isArray(parsed)
			? parsed.filter((n) => n && typeof n.id === "string" && typeof n.text === "string")
			: [];
	} catch {
		return [];
	}
}

export const readNotesView = (value: unknown): NotesView =>
	value === "notes" || value === "todos" ? value : "all";
const readNotesSort = (value: unknown): NotesSort =>
	value === "oldest" || value === "az" || value === "za" ? value : "newest";

export const initialNotesLayout = (): NotesLayout => ({
	view: readNotesView(localStorage.getItem(VIEW_KEY)),
	editorOpen: false
});

/** The sections shown for a view, each with its items. */
function sections(items: NoteItem[], view: NotesView) {
	const notes = items.filter((n) => !n.todo);
	const todos = items.filter((n) => n.todo);
	const shown: { kind: "notes" | "todos"; items: NoteItem[] }[] = [];
	if (view !== "todos") shown.push({ kind: "notes", items: notes });
	if (view !== "notes") shown.push({ kind: "todos", items: todos });
	// In "All", an empty section isn't shown.
	return view === "all" ? shown.filter((s) => s.items.length > 0) : shown;
}

/** Panel height inside the open notch, below its 36 px header. */
export function notesPanelHeight(items: NoteItem[], layout: NotesLayout, scale = 1) {
	if (layout.editorOpen) return notesEditorSize(scale).height - 36;
	const shown = sections(items, layout.view);
	const rows = shown.reduce((sum, s) => sum + s.items.length, 0);
	const headers = layout.view === "all" ? shown.length : 0;
	const content = rows * NOTES_ROW_HEIGHT + headers * HEADER_HEIGHT;
	const list = rows === 0 ? 34 : Math.min(content, LIST_MAX_HEIGHT);
	const clear = items.some((n) => n.done) && layout.view !== "notes" ? 28 : 0;
	// Padding, input row, gap, toolbar, gap, list, "clear done", padding.
	return 12 + 34 + 8 + 22 + 8 + list + clear + 12;
}

function sorted(items: NoteItem[], sort: NotesSort, kind: "notes" | "todos") {
	const list = [...items];
	const by: Record<NotesSort, (a: NoteItem, b: NoteItem) => number> = {
		newest: (a, b) => b.created - a.created,
		oldest: (a, b) => a.created - b.created,
		az: (a, b) => noteLabel(a).localeCompare(noteLabel(b), undefined, { sensitivity: "base" }),
		za: (a, b) => noteLabel(b).localeCompare(noteLabel(a), undefined, { sensitivity: "base" })
	};
	list.sort(by[sort]);
	// Open to-dos come before the ones that are done.
	if (kind === "todos") list.sort((a, b) => Number(a.done) - Number(b.done));
	return list;
}

const newId = () => `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;

interface NotesPanelProps {
	items: NoteItem[];
	onChange: (next: NoteItem[]) => void;
	/** Keeps the notch open while typing. */
	onPinChange: (pinned: boolean) => void;
	/** The view and whether a note is open, for the notch's size. */
	onLayoutChange: (layout: NotesLayout) => void;
}

// Drawn again only when the notes change, not on every redraw of the notch.
export const NotesPanel = memo(function NotesPanel({
	items,
	onChange,
	onPinChange,
	onLayoutChange
}: NotesPanelProps) {
	const [draft, setDraft] = useState("");
	const [todo, setTodo] = useState(() => localStorage.getItem("bloom-notch-notes-kind") !== "note");
	const [editing, setEditing] = useState<string | null>(null);
	/** A note's title is being edited (else a to-do's words). */
	const [editingTitle, setEditingTitle] = useState(false);
	const [editText, setEditText] = useState("");
	const [view, setViewState] = useState<NotesView>(() =>
		readNotesView(localStorage.getItem(VIEW_KEY))
	);
	const [sort, setSortState] = useState<NotesSort>(() =>
		readNotesSort(localStorage.getItem(SORT_KEY))
	);
	const [openNote, setOpenNote] = useState<string | null>(null);

	// The latest list, for the editor's saves (it keeps typing between them).
	const itemsRef = useRef(items);
	itemsRef.current = items;
	/** Hands on a new list made from `itemsRef`, which follows it at once, so
	 * a second change before the next draw builds on this one. */
	const commit = (next: NoteItem[]) => {
		itemsRef.current = next;
		onChange(next);
	};

	useEffect(() => {
		void loadNoteEditor();
	}, []);

	useEffect(() => {
		onLayoutChange({ view, editorOpen: openNote !== null });
	}, [view, openNote]);

	// A note deleted elsewhere (another window) closes its page.
	useEffect(() => {
		if (openNote && !items.some((n) => n.id === openNote)) setOpenNote(null);
	}, [items, openNote]);

	const setView = (next: NotesView) => {
		setViewState(next);
		saveStoredSetting(VIEW_KEY, next);
	};
	const nextSort = () => {
		const next = SORT_ORDER[(SORT_ORDER.indexOf(sort) + 1) % SORT_ORDER.length];
		setSortState(next);
		saveStoredSetting(SORT_KEY, next);
	};

	const add = () => {
		const text = draft.trim();
		if (!text) return;
		onChange([{ id: newId(), text, todo, done: false, created: Date.now() }, ...items]);
		setDraft("");
		// A new item shows in its section.
		if (view !== "all" && (view === "todos") !== todo) setView("all");
	};

	/** Opens a new note as a full page, starting with what was typed. */
	const addFullNote = () => {
		const id = newId();
		onChange([
			{ id, text: draft, todo: false, done: false, created: Date.now() },
			...items
		]);
		setDraft("");
		setOpenNote(id);
	};

	const update = (id: string, change: Partial<NoteItem>) =>
		onChange(items.map((n) => (n.id === id ? { ...n, ...change } : n)));

	const finishEdit = (save: boolean) => {
		if (save && editing) {
			const text = editText.trim();
			// A title left empty goes back to the note's first line.
			if (editingTitle) update(editing, { title: text || undefined });
			else if (text) update(editing, { text, updated: Date.now() });
		}
		setEditing(null);
		setEditingTitle(false);
		onPinChange(false);
	};

	/** Ticks a to-do off (or back on); one made from a note's item ticks or
	 * strikes that item in the note too. */
	const toggleDone = (item: NoteItem) => {
		const done = !item.done;
		const from = item.from;
		onChange(
			items.map((n) => {
				if (n.id === item.id) return { ...n, done };
				if (from && n.id === from.note && n.md) {
					const text = setNoteItemDone(n.text, from.text, done);
					return text === null ? n : { ...n, text };
				}
				return n;
			})
		);
	};

	/** The note's text, and what its editor says about its linked to-dos. */
	const saveFromEditor = (id: string, text: string, sync: TodoSync | null) => {
		const current = itemsRef.current;
		const known = current.find((n) => n.id === id);
		if (!known) return;
		let changed = false;
		let next = current;
		if (!(known.text === text && known.md)) {
			next = next.map((n) => (n.id === id ? { ...n, text, md: true, updated: Date.now() } : n));
			changed = true;
		}
		if (sync) {
			const renamed = new Map(sync.renamed.map((r) => [r.from, r.to]));
			const states = new Map(sync.states.map((s) => [s.text, s.done]));
			next = next.map((n) => {
				if (!n.todo || n.from?.note !== id) return n;
				let item = n;
				const to = renamed.get(n.from.text);
				if (to !== undefined) {
					item = {
						...item,
						// Words changed in the list stay as they were made there.
						text: item.text === n.from.text ? to : item.text,
						from: { note: id, text: to }
					};
				}
				const done = states.get(item.from!.text);
				if (done !== undefined && done !== item.done) item = { ...item, done };
				if (item !== n) changed = true;
				return item;
			});
			const now = Date.now();
			const added: NoteItem[] = sync.add
				.filter(
					(a) =>
						a.text &&
						!next.some((n) => n.todo && n.from?.note === id && n.from.text === a.text)
				)
				.map((a, i) => ({
					id: newId(),
					text: a.text,
					todo: true,
					done: a.done,
					created: now + i,
					from: { note: id, text: a.text }
				}));
			if (added.length) {
				next = [...added, ...next];
				changed = true;
			}
			const before = known.todoLinks ?? [];
			if (before.length !== sync.links.length || before.some((w, i) => w !== sync.links[i])) {
				next = next.map((n) => (n.id === id ? { ...n, todoLinks: sync.links } : n));
				changed = true;
			}
		}
		if (changed) commit(next);
	};

	const shown = useMemo(() => sections(items, view), [items, view]);
	const doneCount = items.filter((n) => n.done).length;
	const counts = {
		notes: items.filter((n) => !n.todo).length,
		todos: items.filter((n) => n.todo).length
	};

	const note = openNote ? items.find((n) => n.id === openNote) : undefined;
	if (note) {
		return (
			<PanelBoundary key={note.id} page="Notes">
			<Suspense fallback={<div className="notes-panel notes-editor" />}>
			<NoteEditor
				key={note.id}
				note={note}
				onPinChange={onPinChange}
				onSave={(text, sync) => saveFromEditor(note.id, text, sync)}
				onTitle={(title) =>
					commit(
						itemsRef.current.map((n) =>
							n.id === note.id ? { ...n, title: title.trim() ? title : undefined } : n
						)
					)
				}
				onAutoTodo={(on) =>
					commit(itemsRef.current.map((n) => (n.id === note.id ? { ...n, autoTodo: on } : n)))
				}
				onClose={(text) => {
					const current = itemsRef.current;
					// A note left empty isn't kept.
					if (!text.trim()) onChange(current.filter((n) => n.id !== note.id));
					setOpenNote(null);
				}}
				onDelete={() => {
					onChange(itemsRef.current.filter((n) => n.id !== note.id));
					setOpenNote(null);
				}}
			/>
			</Suspense>
			</PanelBoundary>
		);
	}

	return (
		<div className="notes-panel" onClick={(e) => e.stopPropagation()}>
			<div className="notes-input-row">
				<button
					type="button"
					className="notes-kind"
					title={todo ? "Adding to-dos (click for notes)" : "Adding notes (click for to-dos)"}
					onClick={() => {
						const next = !todo;
						setTodo(next);
						localStorage.setItem("bloom-notch-notes-kind", next ? "todo" : "note");
					}}
				>
					{todo ? <ListTodo size={15} /> : <StickyNote size={15} />}
				</button>
				<input
					className="notes-input"
					placeholder={todo ? "Add a to-do…" : "Add a note…"}
					value={draft}
					spellCheck={false}
					onChange={(e) => setDraft(e.target.value)}
					onFocus={() => onPinChange(true)}
					onBlur={() => onPinChange(false)}
					onKeyDown={(e) => {
						e.stopPropagation();
						// Shift+Enter writes a note as a full page.
						if (e.key === "Enter" && e.shiftKey && !todo) {
							e.preventDefault();
							addFullNote();
							return;
						}
						if (e.key === "Enter") add();
						if (e.key === "Escape") (e.target as HTMLInputElement).blur();
					}}
				/>
				{!todo && (
					<button
						type="button"
						className="notes-kind"
						title="Write a full note (Shift+Enter)"
						onClick={addFullNote}
					>
						<Maximize2 size={13} />
					</button>
				)}
			</div>

			<div className="notes-toolbar">
				<div className="notes-views" role="tablist">
					{(
						[
							["all", "All"],
							["notes", `Notes ${counts.notes}`],
							["todos", `To-Do ${counts.todos}`]
						] as [NotesView, string][]
					).map(([id, label]) => (
						<button
							key={id}
							type="button"
							role="tab"
							aria-selected={view === id}
							className={`notes-view${view === id ? " active" : ""}`}
							onClick={() => setView(id)}
						>
							{label}
						</button>
					))}
				</div>
				<button
					type="button"
					className="notes-sort"
					title="Change the order"
					onClick={nextSort}
				>
					<ArrowDownUp size={11} />
					{SORT_LABELS[sort]}
				</button>
			</div>

			{shown.every((s) => s.items.length === 0) ? (
				<div className="notes-empty">
					{view === "todos"
						? "No to-dos. Type above and press Enter"
						: view === "notes"
							? "No notes. Type above and press Enter"
							: "Nothing here yet. Type above and press Enter"}
				</div>
			) : (
				<div className="notes-list" style={{ maxHeight: LIST_MAX_HEIGHT }}>
					{shown.map((section) => (
						<div key={section.kind} className="notes-section">
							{view === "all" && (
								<div className="notes-section-title" style={{ height: HEADER_HEIGHT }}>
									{section.kind === "notes" ? "Notes" : "To-Do"}
									<span>{section.items.length}</span>
								</div>
							)}
							{sorted(section.items, sort, section.kind).map((item) => (
								<div
									key={item.id}
									className={`notes-row${item.done ? " done" : ""}`}
									style={{ minHeight: NOTES_ROW_HEIGHT }}
								>
									{item.todo ? (
										<button
											type="button"
											className={`notes-check${item.done ? " checked" : ""}`}
											title={item.done ? "Mark as not done" : "Mark as done"}
											onClick={() => toggleDone(item)}
										>
											{item.done && <Check size={11} strokeWidth={3} />}
										</button>
									) : (
										<span className="notes-dot" />
									)}
									{editing === item.id ? (
										<input
											className="notes-edit"
											autoFocus
											placeholder={
												editingTitle ? noteTitle(item.text, !!item.md) || "Title" : undefined
											}
											value={editText}
											onChange={(e) => setEditText(e.target.value)}
											onFocus={() => onPinChange(true)}
											onBlur={() => finishEdit(true)}
											onKeyDown={(e) => {
												e.stopPropagation();
												if (e.key === "Enter") finishEdit(true);
												if (e.key === "Escape") finishEdit(false);
											}}
										/>
									) : (
										<span
											className="notes-text"
											title={item.todo ? "Double-click to edit" : "Double-click to open"}
											onDoubleClick={() => {
												if (item.todo) {
													setEditing(item.id);
													setEditingTitle(false);
													setEditText(item.text);
												} else {
													setOpenNote(item.id);
												}
											}}
										>
											{(item.todo ? noteTitle(item.text, !!item.md) : noteLabel(item)) || "Empty note"}
											{!item.todo && !item.title?.trim() && noteHasMore(item.text) && (
												<span className="notes-more"> …</span>
											)}
										</span>
									)}
									{!item.todo && editing !== item.id && (
										<button
											type="button"
											className="notes-delete"
											title="Give the note a title"
											onClick={() => {
												setEditing(item.id);
												setEditingTitle(true);
												setEditText(item.title ?? "");
											}}
										>
											<Pencil size={11} />
										</button>
									)}
									<button
										type="button"
										className="notes-delete"
										title="Delete"
										onClick={() => onChange(items.filter((n) => n.id !== item.id))}
									>
										<X size={12} />
									</button>
								</div>
							))}
						</div>
					))}
				</div>
			)}

			{doneCount > 0 && view !== "notes" && (
				<button
					type="button"
					className="notes-clear"
					onClick={() => onChange(items.filter((n) => !n.done))}
				>
					Clear {doneCount} done
				</button>
			)}
		</div>
	);
});
