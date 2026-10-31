import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
	Check,
	ChevronDown,
	Cloud,
	CloudOff,
	ExternalLink,
	Kanban,
	ListTodo,
	Loader2,
	Plus,
	RefreshCw,
	Rocket,
	Zap
} from "lucide-react";
import "./BlitzitPanel.css";

/**
 * Blitzit in the notch, two ways to look at the same list:
 *
 * - The list: the list you're on, its columns as tabs (Today, This week, …),
 *   the tasks of the column picked, and an add row at the bottom. A task
 *   added goes into that list and that column, as in Blitzit itself.
 * - The board (Kanban): every column of the list side by side, an add row
 *   at the bottom of each, tasks dragged from one column to another (into
 *   Done ticks them off). A bit smaller than an open note.
 *
 * Everything shows at once from the copy kept on this PC and works offline:
 * changes wait on this PC and reach Blitzit as soon as it can be reached
 * (see `blitzit_sync.rs`). While the page is open, Blitzit is read again
 * every few seconds, so changes made in the Blitzit app show up too.
 */

export interface BlitzitStatus {
	connected: boolean;
	server: string;
	tools: string[];
	can_add: boolean;
	can_read_lists: boolean;
	can_read_tasks: boolean;
	can_complete: boolean;
	can_move: boolean;
	columns: string[];
	/** Blitzit's tools Bloom uses for adding, lists, tasks, ticking off, moving. */
	using: (string | null)[];
	/** Each of Blitzit's tools and what it says it does. */
	tool_details: [string, string][];
	/** Why Blitzit couldn't be reached or used. */
	problem: string | null;
	app_found: boolean;
}

interface List {
	id: string;
	name: string;
}

interface Task {
	id: string;
	name: string;
	done: boolean;
	column: string;
	/** Not in Blitzit yet. */
	pending: boolean;
}

interface Board {
	connected: boolean;
	lists: List[];
	list: List | null;
	tasks: Task[];
	columns: string[];
	can_add: boolean;
	can_complete: boolean;
	can_move: boolean;
	offline: boolean;
	problem: string | null;
	pending: number;
	synced_at: number | null;
}

interface Column {
	/** What Blitzit calls it ("THIS_WEEK"); "" for tasks with no column. */
	key: string;
	label: string;
	done: boolean;
}

export type BlitzitView = "list" | "board";

const LIST_KEY = "bloom-blitzit-list";
const COLUMN_KEY = "bloom-blitzit-column";
const VIEW_KEY = "bloom-blitzit-view";
const DONE_KEY = "__done";
const POLL_MS = 15_000;
const VISIBLE_TASKS = 5;
const TASK_ROW = 28;
const PICKER_ROWS = 7;
/** The board's share of the screen (an open note is 68% × 63%). */
const BOARD_WIDTH_SHARE = 0.58;
const BOARD_HEIGHT_SHARE = 0.55;

function squash(text: string) {
	return text.toLowerCase().replace(/[^a-z0-9]/g, "");
}

/** "THIS_WEEK" → "This week". */
function pretty(name: string) {
	const words = name.replace(/[_-]+/g, " ").trim();
	if (!words) return name;
	const lower = words === words.toUpperCase() ? words.toLowerCase() : words;
	return lower.charAt(0).toUpperCase() + lower.slice(1);
}

/** Blitzit's own order: Backlog, This week, Today, then anything else. */
function rank(key: string) {
	const k = squash(key);
	if (k.includes("backlog") || k.includes("someday") || k.includes("later")) return 0;
	if (k.includes("week")) return 1;
	if (k.includes("today")) return 2;
	return 1.5;
}

function isDoneName(key: string) {
	return ["done", "completed", "complete", "finished"].includes(squash(key));
}

/** The list's columns: the ones Blitzit names, the ones its tasks are in, and Done. */
function columnsOf(board: Board | null): Column[] {
	if (!board) return [];
	const found: Column[] = [];
	const add = (key: string) => {
		if (isDoneName(key) || found.some((c) => squash(c.key) === squash(key))) return;
		found.push({ key, label: pretty(key), done: false });
	};
	board.columns.forEach(add);
	board.tasks.filter((t) => !t.done && t.column).forEach((t) => add(t.column));
	found.sort((a, b) => rank(a.key) - rank(b.key));
	// Tasks in no column Blitzit names (or no columns at all) still get one.
	if (!found.length || board.tasks.some((t) => !t.done && !columnFor(found, t.column))) {
		found.push({ key: "", label: found.length ? "No column" : "Tasks", done: false });
	}
	found.push({ key: DONE_KEY, label: "Done", done: true });
	return found;
}

function columnFor(columns: Column[], name: string) {
	if (!name) return columns.find((c) => c.key === "" && !c.done);
	return columns.find((c) => !c.done && squash(c.key) === squash(name));
}

function inColumn(task: Task, column: Column, columns: Column[]) {
	if (column.done) return task.done;
	if (task.done) return false;
	return columnFor(columns, task.column) === column;
}

/** The notch's size with the board open, in the notch's own units. */
export function blitzitBoardSize(scale: number) {
	const zoom = scale > 0 ? scale : 1;
	return {
		width: Math.max(640, Math.round((window.screen.width * BOARD_WIDTH_SHARE) / zoom)),
		height: Math.max(380, Math.round((window.screen.height * BOARD_HEIGHT_SHARE) / zoom))
	};
}

/** The list view's height inside the open notch, below its 36 px header. */
export function blitzitPanelHeight(
	connected: boolean,
	taskRows: number,
	message: boolean,
	tabs = false,
	statusLine = false,
	pickerRows = 0,
	problem = false
) {
	let body = 12 + 30;
	if (problem) body += 8 + 34;
	if (connected) {
		if (tabs) body += 8 + 26;
		body += 8 + (taskRows > 0 ? Math.min(taskRows, VISIBLE_TASKS) * TASK_ROW : 30);
		body += 8 + 34;
		if (statusLine || message) body += 6 + 16;
	} else {
		body += 8 + 30;
	}
	if (pickerRows > 0) body = Math.max(body, 12 + 30 + 4 + Math.min(pickerRows, PICKER_ROWS) * 30 + 8);
	return body + 12;
}

function readStored<T>(key: string, fallback: T): T {
	try {
		const raw = localStorage.getItem(key);
		return raw ? (JSON.parse(raw) as T) : fallback;
	} catch {
		return fallback;
	}
}

function saveStored(key: string, value: unknown) {
	const text = JSON.stringify(value);
	try {
		localStorage.setItem(key, text);
	} catch {
		/* the backend keeps it too */
	}
	invoke("save_setting", { key, value: text }).catch(() => {});
}

function syncedText(board: Board | null) {
	if (!board) return "";
	if (board.offline) {
		return board.pending > 0
			? `Offline · ${board.pending} change${board.pending === 1 ? "" : "s"} will sync when you're back online`
			: "Offline · showing your tasks as last synced";
	}
	if (board.pending > 0) return `Syncing ${board.pending} change${board.pending === 1 ? "" : "s"}…`;
	if (!board.synced_at) return "Not synced yet";
	const time = new Date(board.synced_at).toLocaleTimeString([], {
		hour: "2-digit",
		minute: "2-digit",
		hour12: false
	});
	return `Synced with Blitzit at ${time}`;
}

interface DragState {
	task: Task;
	/** Where the card was picked up, in the board's own units. */
	x: number;
	y: number;
	width: number;
	grabX: number;
	grabY: number;
	over: string | null;
}

export function BlitzitPanel({
	onPinChange,
	onHeightChange,
	onOpenSettings,
	onViewChange,
	scale
}: {
	onPinChange: (pinned: boolean) => void;
	onHeightChange: (height: number) => void;
	onOpenSettings: () => void;
	onViewChange: (view: BlitzitView) => void;
	scale: number;
}) {
	const [board, setBoard] = useState<Board | null>(null);
	const [list, setListState] = useState<List | null>(() => readStored<List | null>(LIST_KEY, null));
	const listRef = useRef(list);
	const [columnKey, setColumnKey] = useState<string>(() => readStored<string>(COLUMN_KEY, ""));
	const [view, setViewState] = useState<BlitzitView>(() =>
		readStored<BlitzitView>(VIEW_KEY, "list") === "board" ? "board" : "list"
	);
	const [draft, setDraft] = useState("");
	/** The board column whose add row is open. */
	const [addingIn, setAddingIn] = useState<string | null>(null);
	const [picker, setPicker] = useState(false);
	const [syncing, setSyncing] = useState(false);
	const [busy, setBusy] = useState<string | null>(null);
	const [message, setMessage] = useState<string | null>(null);
	/** Ticked off just now: shown struck through for a moment. */
	const [justDone, setJustDone] = useState<string[]>([]);
	const [drag, setDrag] = useState<DragState | null>(null);
	const dragRef = useRef<DragState | null>(null);
	const boardRef = useRef<HTMLDivElement | null>(null);
	const pressRef = useRef<{ task: Task; x: number; y: number; card: HTMLElement } | null>(null);
	const freshInFlight = useRef(false);
	/** Asked for again while a read was under way (another list was picked). */
	const freshAgain = useRef(false);
	const ticks = useRef(0);

	const setView = (next: BlitzitView) => {
		setViewState(next);
		saveStored(VIEW_KEY, next);
		setAddingIn(null);
		setPicker(false);
	};

	/** Shows a board, unless another list was picked while it was being read. */
	const show = useCallback((next: Board, asked: string | null) => {
		if (asked !== null && asked !== (listRef.current?.id ?? null)) return;
		setBoard(next);
		if (next.list && next.list.id !== listRef.current?.id) {
			listRef.current = next.list;
			setListState(next.list);
			saveStored(LIST_KEY, next.list);
		}
	}, []);

	/** The copy kept here: instant, works offline. */
	const loadSaved = useCallback(async () => {
		const asked = listRef.current?.id ?? null;
		try {
			show(await invoke<Board>("blitzit_board", { listId: asked, fresh: false }), asked);
		} catch (e) {
			setMessage(String(e));
		}
	}, [show]);

	/** Asks Blitzit (and sends what's waiting). */
	const loadFresh = useCallback(
		async (withLists = false) => {
			if (freshInFlight.current) {
				freshAgain.current = true;
				return;
			}
			freshInFlight.current = true;
			setSyncing(true);
			let again = false;
			try {
				do {
					freshAgain.current = false;
					const asked = listRef.current?.id ?? null;
					show(
						await invoke<Board>("blitzit_board", { listId: asked, fresh: true, withLists }),
						asked
					);
					again = freshAgain.current;
				} while (again);
			} catch (e) {
				setMessage(String(e));
			} finally {
				freshInFlight.current = false;
				setSyncing(false);
			}
		},
		[show]
	);

	useEffect(() => {
		void loadSaved().then(() => loadFresh(true));
		const timer = window.setInterval(() => {
			if (document.visibilityState === "hidden" || dragRef.current) return;
			ticks.current += 1;
			void loadFresh(ticks.current % 4 === 0);
		}, POLL_MS);
		const synced = listen("blitzit-synced", () => void loadSaved());
		const failed = listen<string[]>("blitzit-sync-failed", (e) => {
			if (e.payload?.length) setMessage(e.payload[0]);
			void loadSaved();
		});
		const changed = listen("blitzit-changed", () => void loadFresh(true));
		return () => {
			window.clearInterval(timer);
			void synced.then((f) => f());
			void failed.then((f) => f());
			void changed.then((f) => f());
		};
	}, [loadSaved, loadFresh]);

	// Notes like "Couldn't add…" fade after a few seconds.
	useEffect(() => {
		if (!message) return;
		const timer = window.setTimeout(() => setMessage(null), 6000);
		return () => window.clearTimeout(timer);
	}, [message]);

	useEffect(() => {
		if (!justDone.length) return;
		const timer = window.setTimeout(() => setJustDone([]), 2500);
		return () => window.clearTimeout(timer);
	}, [justDone]);

	const columns = useMemo(() => columnsOf(board), [board]);
	const openColumns = columns.filter((c) => !c.done);
	const shownColumn =
		openColumns.find((c) => c.key === columnKey) ??
		openColumns.find((c) => squash(c.key).includes("today")) ??
		openColumns[0] ??
		null;
	const listTasks = (board?.tasks ?? []).filter(
		(t) =>
			shownColumn &&
			(inColumn(t, shownColumn, columns) ||
				(t.done && justDone.includes(t.id) && columnFor(columns, t.column) === shownColumn))
	);
	const connected = !!board?.connected;
	const lists = board?.lists ?? [];
	const statusLine = connected && (!!board?.offline || (board?.pending ?? 0) > 0 || !!board?.problem);

	// The notch grows to the board only once there's a board to show.
	useEffect(() => {
		onViewChange(view === "board" && connected ? "board" : "list");
	}, [view, connected, onViewChange]);

	useEffect(() => {
		if (view === "board" && connected) {
			onHeightChange(blitzitBoardSize(scale).height - 36);
			return;
		}
		onHeightChange(
			blitzitPanelHeight(
				board ? connected : true,
				listTasks.length,
				!!message,
				openColumns.length > 1,
				statusLine,
				picker ? lists.length : 0,
				!!board?.problem
			)
		);
	}, [
		view,
		connected,
		board,
		listTasks.length,
		message,
		openColumns.length,
		statusLine,
		picker,
		lists.length,
		scale,
		onHeightChange
	]);

	const chooseList = (next: List) => {
		setPicker(false);
		if (next.id === listRef.current?.id) return;
		listRef.current = next;
		setListState(next);
		saveStored(LIST_KEY, next);
		void loadSaved().then(() => loadFresh());
	};

	const chooseColumn = (key: string) => {
		setColumnKey(key);
		saveStored(COLUMN_KEY, key);
	};

	const add = async (column: Column | null) => {
		const title = draft.trim();
		if (!title || busy === "add") return;
		setBusy("add");
		try {
			const made = await invoke<Task>("blitzit_add_task", {
				title,
				listId: list?.id ?? null,
				listName: list?.name ?? null,
				column: column && !column.done && column.key ? column.key : null
			});
			setDraft("");
			setBoard((prev) => (prev ? { ...prev, tasks: [...prev.tasks, made], pending: prev.pending + 1 } : prev));
			void loadSaved();
		} catch (e) {
			setMessage(String(e));
		} finally {
			setBusy(null);
		}
	};

	const complete = async (task: Task) => {
		if (task.done) return;
		setJustDone((prev) => [...prev, task.id]);
		setBoard((prev) =>
			prev ? { ...prev, tasks: prev.tasks.map((t) => (t.id === task.id ? { ...t, done: true } : t)) } : prev
		);
		try {
			await invoke("blitzit_complete_task", {
				taskId: task.id,
				listId: list?.id ?? null,
				listName: list?.name ?? null
			});
		} catch (e) {
			setMessage(String(e));
		}
		void loadSaved();
	};

	const move = async (task: Task, column: Column) => {
		if (column.done) {
			void complete(task);
			return;
		}
		setBoard((prev) =>
			prev
				? { ...prev, tasks: prev.tasks.map((t) => (t.id === task.id ? { ...t, column: column.key } : t)) }
				: prev
		);
		try {
			await invoke("blitzit_move_task", {
				taskId: task.id,
				listId: list?.id ?? null,
				listName: list?.name ?? null,
				column: column.key
			});
		} catch (e) {
			setMessage(String(e));
		}
		void loadSaved();
	};

	const start = async () => {
		setBusy("start");
		try {
			const result = await invoke<string>("blitzit_start");
			if (result !== "started") setMessage("Blitzit is open: press Blitzit now there");
		} catch (e) {
			setMessage(String(e));
		} finally {
			setBusy(null);
		}
	};

	// ---------------------------------------------------------------- dragging on the board

	/** The board's own units per screen pixel (the notch is zoomed). */
	const boardScale = () => {
		const el = boardRef.current;
		if (!el || !el.offsetWidth) return 1;
		return el.getBoundingClientRect().width / el.offsetWidth;
	};

	const columnAt = (x: number, y: number) => {
		const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-column]");
		return el?.dataset.column ?? null;
	};

	const onCardDown = (e: React.PointerEvent<HTMLElement>, task: Task) => {
		if (e.button !== 0 || task.done || !board?.can_move) return;
		if ((e.target as HTMLElement).closest("button")) return;
		pressRef.current = { task, x: e.clientX, y: e.clientY, card: e.currentTarget };
		e.currentTarget.setPointerCapture(e.pointerId);
	};

	const onCardMove = (e: React.PointerEvent<HTMLElement>) => {
		const press = pressRef.current;
		if (!press) return;
		const k = boardScale();
		const el = boardRef.current;
		const root = el?.getBoundingClientRect();
		if (!el || !root) return;
		// Positions inside the board, which may be scrolled sideways.
		const at = (clientX: number, clientY: number) => ({
			x: (clientX - root.left) / k + el.scrollLeft,
			y: (clientY - root.top) / k + el.scrollTop
		});
		if (!dragRef.current) {
			if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 5) return;
			const card = press.card.getBoundingClientRect();
			const next: DragState = {
				task: press.task,
				...at(e.clientX, e.clientY),
				width: card.width / k,
				grabX: (press.x - card.left) / k,
				grabY: (press.y - card.top) / k,
				over: columnAt(e.clientX, e.clientY)
			};
			dragRef.current = next;
			setDrag(next);
			onPinChange(true);
			return;
		}
		const next = {
			...dragRef.current,
			...at(e.clientX, e.clientY),
			over: columnAt(e.clientX, e.clientY)
		};
		dragRef.current = next;
		setDrag(next);
	};

	const endDrag = (drop: boolean) => {
		const current = dragRef.current;
		pressRef.current = null;
		dragRef.current = null;
		setDrag(null);
		if (!current) return;
		onPinChange(false);
		if (!drop || current.over === null) return;
		const target = columns.find((c) => (c.done ? DONE_KEY : c.key) === current.over);
		if (target && !inColumn(current.task, target, columns)) void move(current.task, target);
	};

	// ---------------------------------------------------------------- parts

	const checkButton = (task: Task) => (
		<button
			type="button"
			className="blitzit-check"
			title={board?.can_complete ? "Mark done" : "Blitzit can't tick tasks off from here"}
			disabled={!board?.can_complete || task.done}
			onClick={(e) => {
				e.stopPropagation();
				void complete(task);
			}}
		>
			{task.done && <Check />}
		</button>
	);

	const pendingMark = (task: Task) =>
		task.pending && (
			<span className="blitzit-pending" title="Waiting to reach Blitzit">
				<CloudOff />
			</span>
		);

	const addRow = (column: Column | null, autoFocus: boolean, onDone?: () => void) => (
		<div className="blitzit-add">
			<Plus className="blitzit-add-mark" />
			<input
				className="blitzit-input"
				value={draft}
				autoFocus={autoFocus}
				placeholder={
					column && column.key
						? `Add a task to ${column.label}${list ? ` in ${list.name}` : ""}`
						: `Add a task${list ? ` to ${list.name}` : ""}`
				}
				disabled={busy === "add"}
				onChange={(e) => setDraft(e.target.value)}
				onFocus={() => onPinChange(true)}
				onBlur={() => {
					onPinChange(false);
					if (!draft.trim()) onDone?.();
				}}
				onKeyDown={(e) => {
					e.stopPropagation();
					if (e.key === "Enter") void add(column);
					if (e.key === "Escape") {
						setDraft("");
						(e.target as HTMLInputElement).blur();
						onDone?.();
					}
				}}
			/>
			{busy === "add" && <Loader2 className="blitzit-spin blitzit-add-mark" />}
		</div>
	);

	const listButton = (
		<button
			type="button"
			className={`blitzit-list-button${picker ? " is-open" : ""}`}
			title="The list you're on: tasks you add go here"
			disabled={lists.length === 0}
			onClick={() => setPicker((open) => !open)}
		>
			<span className="blitzit-list-name">{list?.name ?? "Blitzit"}</span>
			{lists.length > 1 && <ChevronDown />}
		</button>
	);

	const syncButton = connected && (
		<button
			type="button"
			className={`blitzit-icon${board?.offline ? " is-offline" : ""}`}
			title={`${syncedText(board)}. Click to sync now`}
			disabled={syncing}
			onClick={() => void loadFresh(true)}
		>
			{syncing ? (
				<RefreshCw className="blitzit-spin" />
			) : board?.offline ? (
				<CloudOff />
			) : board && board.pending > 0 ? (
				<RefreshCw />
			) : (
				<Cloud />
			)}
		</button>
	);

	const top = (
		<div className="blitzit-top">
			<Zap className="blitzit-mark" />
			{listButton}
			<span className="blitzit-spacer" />
			{syncButton}
			{connected && (
				<div className="blitzit-views" role="group" aria-label="View">
					<button
						type="button"
						className={view === "list" ? "is-on" : ""}
						title="List: add tasks to the list you're on"
						onClick={() => setView("list")}
					>
						<ListTodo />
					</button>
					<button
						type="button"
						className={view === "board" ? "is-on" : ""}
						title="Board: every column side by side, drag tasks between them"
						onClick={() => setView("board")}
					>
						<Kanban />
					</button>
				</div>
			)}
			<button
				type="button"
				className="blitzit-icon"
				title="Open Blitzit"
				onClick={() => void invoke("blitzit_open").catch((e) => setMessage(String(e)))}
			>
				<ExternalLink />
			</button>
			<button
				type="button"
				className="blitzit-start"
				title="Start Blitz mode in the Blitzit app"
				disabled={busy === "start"}
				onClick={() => void start()}
			>
				{busy === "start" ? <Loader2 className="blitzit-spin" /> : <Rocket />}
				Blitz now
			</button>
		</div>
	);

	const pickerMenu = picker && (
		<div className="blitzit-picker" role="listbox">
			{lists.map((l) => (
				<button
					key={l.id}
					type="button"
					role="option"
					aria-selected={l.id === list?.id}
					className={`blitzit-picker-row${l.id === list?.id ? " is-on" : ""}`}
					onClick={() => chooseList(l)}
				>
					<span>{l.name}</span>
					{l.id === list?.id && <Check />}
				</button>
			))}
		</div>
	);

	const problemRow = board?.problem && (
		<button type="button" className="blitzit-message is-action is-warning" onClick={onOpenSettings}>
			Blitzit: {board.problem}
		</button>
	);

	const statusRow = (statusLine || message) && (
		<div className={`blitzit-status${message ? " is-message" : ""}`} role="status">
			{message ?? syncedText(board)}
		</div>
	);

	if (board && !connected) {
		return (
			<div className="blitzit-panel" onClick={(e) => e.stopPropagation()}>
				{top}
				<button type="button" className="blitzit-message is-action" onClick={onOpenSettings}>
					Connect your Blitzit account in Settings to see and add tasks here
				</button>
			</div>
		);
	}

	if (view === "board") {
		return (
			<div
				className={`blitzit-panel is-board${drag ? " is-dragging" : ""}`}
				onClick={(e) => e.stopPropagation()}
			>
				{top}
				{problemRow}
				<div className="blitzit-board" ref={boardRef}>
					{columns.map((column) => {
						const id = column.done ? DONE_KEY : column.key;
						const tasks = (board?.tasks ?? []).filter((t) => inColumn(t, column, columns));
						return (
							<div
								key={id || "none"}
								data-column={id}
								className={`blitzit-column${drag?.over === id ? " is-over" : ""}${column.done ? " is-done" : ""}`}
							>
								<div className="blitzit-column-head">
									<span>{column.label}</span>
									<span className="blitzit-count">{tasks.length}</span>
								</div>
								<div className="blitzit-cards">
									{tasks.map((task) => (
										<div
											key={task.id}
											className={`blitzit-card${task.done ? " is-done" : ""}${drag?.task.id === task.id ? " is-lifted" : ""}${board?.can_move && !task.done ? " can-drag" : ""}`}
											onPointerDown={(e) => onCardDown(e, task)}
											onPointerMove={onCardMove}
											onPointerUp={() => endDrag(true)}
											onPointerCancel={() => endDrag(false)}
										>
											{checkButton(task)}
											<span className="blitzit-card-name">{task.name}</span>
											{pendingMark(task)}
										</div>
									))}
									{tasks.length === 0 && !column.done && (
										<div className="blitzit-empty">No tasks</div>
									)}
								</div>
								{!column.done &&
									board?.can_add &&
									(addingIn === id ? (
										addRow(column, true, () => setAddingIn(null))
									) : (
										<button
											type="button"
											className="blitzit-add-button"
											onClick={() => {
												setDraft("");
												setAddingIn(id);
											}}
										>
											<Plus /> Add task
										</button>
									))}
							</div>
						);
					})}
					{drag && (
						<div
							className="blitzit-card blitzit-ghost"
							style={{
								left: drag.x - drag.grabX,
								top: drag.y - drag.grabY,
								width: drag.width
							}}
						>
							<span className="blitzit-check" />
							<span className="blitzit-card-name">{drag.task.name}</span>
						</div>
					)}
				</div>
				{statusRow}
				{pickerMenu}
			</div>
		);
	}

	return (
		<div className="blitzit-panel" onClick={(e) => e.stopPropagation()}>
			{top}
			{problemRow}
			{openColumns.length > 1 && (
				<div className="blitzit-tabs" role="tablist">
					{openColumns.map((column) => {
						const count = (board?.tasks ?? []).filter((t) => inColumn(t, column, columns)).length;
						return (
							<button
								key={column.key || "none"}
								type="button"
								role="tab"
								aria-selected={column === shownColumn}
								className={column === shownColumn ? "is-on" : ""}
								onClick={() => chooseColumn(column.key)}
							>
								{column.label}
								<span className="blitzit-count">{count}</span>
							</button>
						);
					})}
				</div>
			)}
			<div className="blitzit-tasks" style={{ maxHeight: VISIBLE_TASKS * TASK_ROW }}>
				{listTasks.map((task) => (
					<div key={task.id} className={`blitzit-task${task.done ? " is-done" : ""}`}>
						{checkButton(task)}
						<span className="blitzit-task-name">{task.name}</span>
						{pendingMark(task)}
					</div>
				))}
				{listTasks.length === 0 && (
					<div className="blitzit-empty">
						{board ? `Nothing in ${shownColumn?.label ?? "this list"} yet` : "Loading your tasks…"}
					</div>
				)}
			</div>
			{board?.can_add === false ? (
				<button type="button" className="blitzit-message is-action is-warning" onClick={onOpenSettings}>
					Bloom didn't recognise Blitzit's add-task action. Pick it in Settings &gt; Integrations &gt; Blitzit
				</button>
			) : (
				addRow(shownColumn, false)
			)}
			{statusRow}
			{pickerMenu}
		</div>
	);
}
