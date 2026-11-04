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
import { BlitzitBoard } from "./BlitzitBoard";
import {
	columnFor,
	columnsOf,
	inColumn,
	squash,
	syncedText,
	type Board,
	type Column,
	type List,
	type Task
} from "./blitzitModel";
import { useClock24h } from "../hooks/useClock24h";
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

export type BlitzitView = "list" | "board";

const LIST_KEY = "bloom-blitzit-list";
const COLUMN_KEY = "bloom-blitzit-column";
const VIEW_KEY = "bloom-blitzit-view";
const POLL_MS = 15_000;
const VISIBLE_TASKS = 5;
const TASK_ROW = 28;
const PICKER_ROWS = 7;
/** The board's share of the screen (an open note is 68% × 63%). */
const BOARD_WIDTH_SHARE = 0.58;
const BOARD_HEIGHT_SHARE = 0.55;

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
	const h24 = useClock24h();
	const [board, setBoard] = useState<Board | null>(null);
	const [list, setListState] = useState<List | null>(() => readStored<List | null>(LIST_KEY, null));
	const listRef = useRef(list);
	const [columnKey, setColumnKey] = useState<string>(() => readStored<string>(COLUMN_KEY, ""));
	const [view, setViewState] = useState<BlitzitView>(() =>
		readStored<BlitzitView>(VIEW_KEY, "list") === "board" ? "board" : "list"
	);
	const [draft, setDraft] = useState("");
	const [picker, setPicker] = useState(false);
	const [syncing, setSyncing] = useState(false);
	const [busy, setBusy] = useState<string | null>(null);
	const [message, setMessage] = useState<string | null>(null);
	/** Ticked off just now: shown struck through for a moment. */
	const [justDone, setJustDone] = useState<string[]>([]);
	/** Something is being dragged or typed: the page isn't read again meanwhile. */
	const holdRef = useRef(false);
	const hold = useCallback(
		(pinned: boolean) => {
			holdRef.current = pinned;
			onPinChange(pinned);
		},
		[onPinChange]
	);
	const freshInFlight = useRef(false);
	/** Asked for again while a read was under way (another list was picked). */
	const freshAgain = useRef(false);
	const ticks = useRef(0);

	const setView = (next: BlitzitView) => {
		setViewState(next);
		saveStored(VIEW_KEY, next);
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
			if (document.visibilityState === "hidden" || holdRef.current) return;
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

	/** The list a task is in (what a change to it is sent with). */
	const listOf = (task: Task) => {
		const id = task.list_id || list?.id || null;
		return { listId: id, listName: board?.lists.find((l) => l.id === id)?.name ?? list?.name ?? null };
	};

	/** Adds a task; true once it's in (shown at once, synced when it can be). */
	const addTask = async (title: string, column: Column | null) => {
		if (!title.trim() || busy === "add") return false;
		setBusy("add");
		try {
			const made = await invoke<Task>("blitzit_add_task", {
				title,
				listId: list?.id ?? null,
				listName: list?.name ?? null,
				column: column && !column.done && column.key ? column.key : null
			});
			setBoard((prev) => (prev ? { ...prev, tasks: [...prev.tasks, made], pending: prev.pending + 1 } : prev));
			void loadSaved();
			return true;
		} catch (e) {
			setMessage(String(e));
			return false;
		} finally {
			setBusy(null);
		}
	};

	const add = async (column: Column | null) => {
		if (await addTask(draft.trim(), column)) setDraft("");
	};

	const complete = async (task: Task) => {
		if (task.done) return;
		setJustDone((prev) => [...prev, task.id]);
		setBoard((prev) =>
			prev ? { ...prev, tasks: prev.tasks.map((t) => (t.id === task.id ? { ...t, done: true } : t)) } : prev
		);
		try {
			await invoke("blitzit_complete_task", { taskId: task.id, ...listOf(task) });
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
				? { ...prev, tasks: prev.tasks.map((t) => (t.id === task.id ? { ...t, column: column.key, done: false } : t)) }
				: prev
		);
		try {
			await invoke("blitzit_move_task", { taskId: task.id, ...listOf(task), column: column.key });
		} catch (e) {
			setMessage(String(e));
		}
		void loadSaved();
	};

	/** Puts a task before `before` (or after `after`) in its column. */
	const reorder = async (task: Task, before: Task | null, after: Task | null) => {
		setBoard((prev) => {
			if (!prev) return prev;
			const rest = prev.tasks.filter((t) => t.id !== task.id);
			const anchor = before ?? after;
			let at = anchor ? rest.findIndex((t) => t.id === anchor.id) : 0;
			if (at < 0) at = rest.length;
			else if (!before && after) at += 1;
			const moved = prev.tasks.find((t) => t.id === task.id) ?? task;
			return { ...prev, tasks: [...rest.slice(0, at), moved, ...rest.slice(at)] };
		});
		try {
			await invoke("blitzit_reorder_task", {
				taskId: task.id,
				...listOf(task),
				before: before?.id ?? null,
				after: before ? null : (after?.id ?? null)
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
			title={`${syncedText(board, h24)}. Click to sync now`}
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
			{message ?? syncedText(board, h24)}
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
			<BlitzitBoard
				board={board}
				list={list}
				syncing={syncing}
				busy={busy}
				message={message}
				onChooseList={chooseList}
				onAdd={addTask}
				onComplete={(task) => void complete(task)}
				onMove={(task, column) => void move(task, column)}
				onReorder={(task, before, after) => void reorder(task, before, after)}
				onSync={() => void loadFresh(true)}
				onList={() => setView("list")}
				onOpen={() => void invoke("blitzit_open").catch((e) => setMessage(String(e)))}
				onStart={() => void start()}
				onPinChange={hold}
				onOpenSettings={onOpenSettings}
			/>
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
