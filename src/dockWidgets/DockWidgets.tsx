import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Check, Layers, Plus, X } from "lucide-react";
import { EdgeSurface } from "../components/EdgeSurface";
import { useSettingsSync } from "../hooks/useSettingsSync";
import { useClock24h } from "../hooks/useClock24h";
import { useWeather } from "../hooks/useWeather";
import { useMedia, useMetrics, useNow, useTodayTasks } from "./data";
import {
	LAYOUT_KEY,
	SPAN,
	WIDGETS,
	fits,
	freeCells,
	fromVersion1,
	visibility,
	gridFor,
	newId,
	placeOf,
	readLayout,
	shelves,
	widgetInfo,
	type Grid,
	type DesktopRule,
	type Layout,
	type Visibility,
	type Placed,
	type Side,
	type WidgetKind,
	type WidgetSize
} from "./model";
import { WidgetFace as Widget, ZONES, readFocus, writeFocus, type WidgetData } from "./widgets";
import "./DockWidgets.css";

/**
 * Widgets in the strips beside the dock (see model.ts for the layout).
 *
 * Each side shows its widgets on shelves like the dock: neighbours share one,
 * attached to the screen's bottom edge. They come and go with the dock (hidden
 * over full-screen apps, and in smart mode while the window in front covers
 * that side, until the dock is called up). Right-click a widget for its menu;
 * Edit widgets (there, or in the desktop's menu beside the dock) shows the grid
 * and a picker to drag widgets from. Dropping a widget onto another makes a
 * group: the mouse wheel flips through it, and it can turn on its own.
 */

interface Props {
	dockRef: RefObject<HTMLDivElement | null>;
	scale: number;
	/** The dock is out of sight (hidden, not ready, or a full-screen app). */
	hidden: boolean;
	/** Smart mode with nothing calling the dock up: a covered side hides. */
	stepAside: boolean;
	/** The dock is at rest, full size (not hidden, shrunk or stretched): its height sizes the grid. */
	resting: boolean;
}

/** A widget's choices for the desktop (its menu): the rule, its label, what it does. */
const DESKTOP_RULES: [DesktopRule, string, string][] = [
	["smart", "Smart", "Hidden in calm moments and while the notch shows the same; pointing at its spot shows it"],
	["show", "Always", "Always shown"],
	["hide", "Hide", "Hidden while the desktop is in front; pointing doesn't show it"]
];

/** Smart's calm moments (a widget's menu). */
const SMART_HIDES: [undefined | "desktop", string, string][] = [
	["desktop", "Desktop", "Only while the desktop is in front"],
	[undefined, "Desktop + small windows", "Also while a window that leaves the notch on screen is in front"]
];

/** How long smart widgets stay after the pointer leaves them. */
const REVEAL_GRACE_MS = 700;

/** The dock's height at rest, kept so the grid is right from the first frame (a cache, not a setting). */
const REST_HEIGHT_KEY = "bloom-dock-widgets-rest-height";

interface Drag {
	/**
	 * What's dragged: a new widget from the picker (`kind`), a placed item with
	 * everything in it (`itemId`), or one widget pulled out of a group (`itemId`
	 * and `kind`).
	 */
	kind?: WidgetKind;
	itemId?: string;
	fromSide?: Side;
	size: WidgetSize;
	/** Where the pointer went down, where it is, and whether it has left. */
	startX: number;
	startY: number;
	x: number;
	y: number;
	moved: boolean;
	/** Where in the widget it was picked up, so it stays under the pointer there. */
	grabX: number;
	grabY: number;
}

/** Where a drag would land: a spot on the grid, or a widget to group with. */
type Landing =
	| { side: Side; group: Placed; cell?: undefined; size?: undefined; ok: boolean }
	| { side: Side; cell: number; size: WidgetSize; ok: boolean; group?: undefined };

/** A wheel gesture over a group (the notch's own rule: one page per gesture). */
const WHEEL_GESTURE_GAP_MS = 220;
const WHEEL_PAGE_DISTANCE = 60;

interface Menu {
	side: Side;
	itemId: string;
	x: number;
	y: number;
}

const ROTATIONS = [0, 5, 10, 30, 60];
const FOCUS_LENGTHS = [15, 25, 45, 60];

/** Only what it's given: the dock's own frequent renders don't reach it. */
export const DockWidgets = memo(DockWidgetsInner);

function DockWidgetsInner({ dockRef, scale, hidden, stepAside, resting }: Props) {
	const [layout, setLayout] = useState<Layout>(() => readLayout(localStorage.getItem(LAYOUT_KEY)));
	const [editing, setEditing] = useState(false);
	const [covered, setCovered] = useState<[boolean, boolean]>([false, false]);
	const [shown, setShown] = useState<Record<string, number>>({});
	const [drag, setDrag] = useState<Drag | null>(null);
	const [menu, setMenu] = useState<Menu | null>(null);
	/** While editing, the group whose widgets are listed above it (the last one pointed at). */
	const [openGroup, setOpenGroup] = useState<string | null>(null);
	const pickerRef = useRef<HTMLDivElement | null>(null);
	const menuRef = useRef<HTMLDivElement | null>(null);
	const hovered = useRef<string | null>(null);
	const wheel = useRef<Record<string, { last: number; distance: number; turned: boolean }>>({});

	// ---- the saved layout
	useEffect(() => {
		void invoke<Record<string, unknown>>("load_settings")
			.then((s) => {
				const text = s?.[LAYOUT_KEY];
				if (typeof text === "string") setLayout(readLayout(text));
			})
			.catch(() => {});
	}, []);
	useSettingsSync({ [LAYOUT_KEY]: (v) => setLayout(readLayout(typeof v === "string" ? v : JSON.stringify(v ?? ""))) });
	const save = useCallback((next: Layout) => {
		setLayout(next);
		const text = JSON.stringify(next);
		try {
			localStorage.setItem(LAYOUT_KEY, text);
		} catch {}
		void invoke("save_setting", { key: LAYOUT_KEY, value: text }).catch(() => {});
	}, []);

	// ---- the grid: fixed to the screen's edges, its cells the dock's height
	// at rest (kept, so it's right from the first frame). Where the dock is now
	// only says how many cells from each edge are free of it; nothing the dock
	// does moves or resizes a widget.
	const [restHeight, setRestHeight] = useState<number | null>(() => {
		const kept = Number(localStorage.getItem(REST_HEIGHT_KEY));
		return kept >= 30 ? kept : null;
	});
	const [width, setWidth] = useState(window.innerWidth);
	const grid: Grid | null = useMemo(
		() => (restHeight ? gridFor(restHeight, width, scale) : null),
		[restHeight, width, scale]
	);
	const [free, setFree] = useState<Record<Side, number> | null>(null);
	const gridRef = useRef(grid);
	gridRef.current = grid;
	const restingRef = useRef(resting);
	restingRef.current = resting;
	/** The dock's edges now (for moving a version 1 layout over). */
	const dockEdges = useRef<{ left: number; right: number } | null>(null);
	useLayoutEffect(() => {
		let heightTimer = 0;
		let freeTimer = 0;
		let pendingHeight = 0;
		let pendingFree = "";
		const measure = () => {
			if (window.innerWidth !== width) setWidth(window.innerWidth);
			const el = dockRef.current;
			if (!el) return;
			// Its layout size (not the springs' transforms) at its scale, about its middle.
			const r = el.getBoundingClientRect();
			const middle = r.left + r.width / 2;
			const w = el.offsetWidth * scale;
			const h = el.offsetHeight * scale;
			dockEdges.current = { left: middle - w / 2, right: middle + w / 2 };
			// Its height at rest, once it has held still a moment.
			if (restingRef.current && h >= 30 && Math.abs(h - pendingHeight) >= 0.5) {
				pendingHeight = h;
				window.clearTimeout(heightTimer);
				heightTimer = window.setTimeout(() => {
					setRestHeight((prev) => {
						if (prev !== null && Math.abs(prev - pendingHeight) < 0.5) return prev;
						try {
							localStorage.setItem(REST_HEIGHT_KEY, String(pendingHeight));
						} catch {}
						return pendingHeight;
					});
				}, 400);
			}
			// Cells free of it, once a spring has settled (no blinking as it overshoots).
			const g = gridRef.current;
			if (!g) return;
			const next = freeCells(g, dockEdges.current);
			const key = `${next.left},${next.right}`;
			if (key === pendingFree) return;
			const first = pendingFree === "";
			pendingFree = key;
			window.clearTimeout(freeTimer);
			if (first) {
				setFree(next);
				return;
			}
			freeTimer = window.setTimeout(
				() => setFree((prev) => (prev && prev.left === next.left && prev.right === next.right ? prev : next)),
				150
			);
		};
		measure();
		const observer = new ResizeObserver(measure);
		if (dockRef.current) observer.observe(dockRef.current);
		window.addEventListener("resize", measure);
		const timer = window.setInterval(measure, 1000);
		return () => {
			observer.disconnect();
			window.removeEventListener("resize", measure);
			window.clearInterval(timer);
			window.clearTimeout(heightTimer);
			window.clearTimeout(freeTimer);
		};
	}, [dockRef, scale, grid, width]);
	const usable = (side: Side) => (grid && free ? free[side] : 0);

	// A layout from before (cells from the dock outward) moves to the fixed
	// grid once, each widget to where it was on the screen.
	useEffect(() => {
		if (layout.version !== 1 || !grid || !dockEdges.current || !resting) return;
		save(fromVersion1(layout, grid, dockEdges.current));
	}, [layout, grid, resting]);

	// ---- covered by the window in front (smart mode), and edit requests
	useEffect(() => {
		const a = listen<[boolean, boolean]>("dock-widgets-covered", (e) => setCovered(e.payload));
		const b = listen("dock-widgets-edit", () => setEditing(true));
		const c = () => setEditing(true);
		window.addEventListener("dw-edit", c);
		return () => {
			void a.then((f) => f());
			void b.then((f) => f());
			window.removeEventListener("dw-edit", c);
		};
	}, []);
	const sideHidden = (side: Side) =>
		!editing && (hidden || (stepAside && covered[side === "left" ? 0 : 1]));

	// ---- on the desktop: what the notch shows, and whether the desktop is in front
	const [notch, setNotch] = useState<{ shown: boolean; kinds: ReadonlySet<string> }>({ shown: false, kinds: new Set() });
	const [onDesktop, setOnDesktop] = useState(false);
	useEffect(() => {
		const a = listen<{ shown: boolean; kinds: string[] }>("notch-shows", (e) =>
			setNotch((prev) => {
				const kinds = new Set(e.payload.kinds);
				const same =
					prev.shown === e.payload.shown && prev.kinds.size === kinds.size && [...kinds].every((k) => prev.kinds.has(k));
				return same ? prev : { shown: e.payload.shown, kinds };
			})
		);
		const b = listen<boolean>("dock-on-desktop", (e) => setOnDesktop(e.payload));
		return () => {
			void a.then((f) => f());
			void b.then((f) => f());
		};
	}, []);
	// Smart widgets hidden come back while their side is pointed at, and go
	// again a moment after the pointer leaves.
	const [revealed, setRevealed] = useState<Record<Side, boolean>>({ left: false, right: false });
	const unreveal = useRef<Record<Side, number>>({ left: 0, right: 0 });
	const revealedRef = useRef(revealed);
	revealedRef.current = revealed;
	const reveal = (side: Side) => {
		window.clearTimeout(unreveal.current[side]);
		unreveal.current[side] = 0;
		if (!revealedRef.current[side]) setRevealed((r) => ({ ...r, [side]: true }));
	};
	// Once, from when the pointer left (moving on elsewhere doesn't hold it).
	const leave = (side: Side) => {
		if (!revealedRef.current[side] || unreveal.current[side]) return;
		unreveal.current[side] = window.setTimeout(() => {
			unreveal.current[side] = 0;
			setRevealed((r) => (r[side] ? { ...r, [side]: false } : r));
		}, REVEAL_GRACE_MS);
	};
	const visOf = (side: Side, item: Placed): Visibility => {
		const vis = visibility(item, editing, notch, onDesktop);
		return vis === "smart" && revealed[side] ? "shown" : vis;
	};

	// ---- what the placed widgets need
	const placed = useMemo(
		() => new Set([...layout.left, ...layout.right].flatMap((p) => p.widgets)),
		[layout]
	);
	const metrics = useMetrics(placed);
	const weather = useWeather(placed.has("weather"));
	const media = useMedia(placed.has("media"));
	const today = useTodayTasks(placed.has("tasks"));
	const ticking = ["clock", "worldclock", "focus", "date", "countdown", "progress"].some((k) => placed.has(k as WidgetKind));
	const now = useNow(ticking, 1000);
	const h24 = useClock24h();
	// The same object while nothing in it changes, so a widget can tell.
	const weatherData = useMemo(
		() => ({
			temperature: weather.temperature,
			condition: weather.weatherCondition,
			icon: weather.weatherIcon,
			city: weather.cityName,
			unit: weather.tempUnit === "fahrenheit" ? "F" : "C"
		}),
		[weather.temperature, weather.weatherCondition, weather.weatherIcon, weather.cityName, weather.tempUnit]
	);
	const data: WidgetData = { metrics, weather: weatherData, media, today, now, h24 };

	// ---- groups: the wheel flips, and a group can turn on its own
	const flip = (item: Placed, by: number) =>
		setShown((s) => ({
			...s,
			[item.id]: ((s[item.id] ?? 0) + by + item.widgets.length) % item.widgets.length
		}));
	// As the notch: a gesture (events closer than a pause) turns one page, once
	// it has gone far enough, so a fast spin or a trackpad flick doesn't skip.
	const onWheel = (item: Placed, e: React.WheelEvent) => {
		if (item.widgets.length < 2) return;
		e.stopPropagation();
		const gesture = (wheel.current[item.id] ??= { last: 0, distance: 0, turned: false });
		const now = performance.now();
		if (now - gesture.last > WHEEL_GESTURE_GAP_MS) {
			gesture.distance = 0;
			gesture.turned = false;
		}
		gesture.last = now;
		if (gesture.turned) return;
		const raw = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
		const unit = e.deltaMode === 1 ? 40 : e.deltaMode === 2 ? 800 : 1;
		gesture.distance += raw * unit;
		if (Math.abs(gesture.distance) < WHEEL_PAGE_DISTANCE) return;
		flip(item, gesture.distance > 0 ? 1 : -1);
		gesture.distance = 0;
		gesture.turned = true;
	};
	useEffect(() => {
		const turning = [...layout.left, ...layout.right].filter((p) => p.widgets.length > 1 && p.rotate > 0);
		if (!turning.length) return;
		const timers = turning.map((item) =>
			window.setInterval(() => {
				if (hovered.current !== item.id) flip(item, 1);
			}, item.rotate * 1000)
		);
		return () => timers.forEach((t) => window.clearInterval(t));
	}, [layout]);

	// ---- where the shelves are: they take clicks (and the dock window's
	// whole lower band while editing, for the picker and dragging)
	// A shelf whose widgets come and go together keeps its shape and slides
	// away and back; only one mixing shown and hidden widgets is split.
	const shelfRects = useMemo(() => {
		// `smart`: its widgets are smart ones stepping aside now (shown or not), so
		// pointing at its spot keeps them or brings them back.
		type Shelf = { side: Side; items: Placed[]; left: number; width: number; vis: Visibility; key: string; smart: boolean };
		if (!grid) return [] as Shelf[];
		const out: Shelf[] = [];
		const push = (side: Side, run: Placed[], vis: Visibility, key: string) => {
			const places = run.map((p) => placeOf(grid, side, p));
			const left = Math.min(...places.map((p) => p.left)) - grid.pad;
			const right = Math.max(...places.map((p) => p.left + p.width)) + grid.pad;
			const smart = run.every((p) => visibility(p, editing, notch, onDesktop) === "smart");
			out.push({ side, items: run, left, width: right - left, vis, key, smart });
		};
		for (const side of ["left", "right"] as Side[]) {
			// What the dock reaches into steps aside (it stays where it was placed).
			const items = layout[side].filter((p) => p.cell + SPAN[p.size] <= usable(side));
			for (const run of shelves(items)) {
				const vis = run.map((p) => visOf(side, p));
				const whole = run.map((p) => p.id).join("-");
				if (vis.every((v) => v === vis[0])) {
					push(side, run, vis[0], whole);
					continue;
				}
				let start = 0;
				for (let i = 1; i <= run.length; i++) {
					if (i === run.length || vis[i] !== vis[start]) {
						const part = run.slice(start, i);
						push(side, part, vis[start], `${whole}:${part.map((p) => p.id).join("-")}`);
						start = i;
					}
				}
			}
		}
		return out;
	}, [grid, layout, free, editing, notch, onDesktop, revealed]);

	useEffect(() => {
		if (!grid) return;
		const height = grid.cell + 2 * grid.pad;
		// Shown shelves, and the spots of smart ones (pointing there brings them back).
		const rects = shelfRects
			.filter((s) => !sideHidden(s.side) && s.vis !== "gone")
			.map((s) => ({
				x: Math.round(s.left),
				y: Math.round(window.innerHeight - height),
				width: Math.round(s.width),
				height: Math.round(height)
			}));
		void invoke("set_dock_widget_rects", { rects }).catch(() => {});
	}, [shelfRects, grid, hidden, stepAside, covered, editing]);

	// Pointing at a smart shelf's spot (shown or not) brings its side's smart
	// widgets back, and they go a moment after the pointer leaves: by where
	// the pointer is, not by the shelves' own enter and leave, which a shelf
	// sliding in under a still pointer doesn't get.
	useEffect(() => {
		if (!grid) return;
		const top = window.innerHeight - (grid.cell + 2 * grid.pad) - 4;
		const spots = shelfRects.filter((s) => s.smart && !sideHidden(s.side));
		const over = (side: Side, x: number, y: number) =>
			y >= top && spots.some((s) => s.side === side && x >= s.left - 4 && x <= s.left + s.width + 4);
		const move = (e: PointerEvent) => {
			for (const side of ["left", "right"] as Side[]) {
				if (over(side, e.clientX, e.clientY)) reveal(side);
				else leave(side);
			}
		};
		// The window lets the pointer through once it's off every shelf.
		const out = () => {
			leave("left");
			leave("right");
		};
		window.addEventListener("pointermove", move);
		document.documentElement.addEventListener("pointerleave", out);
		return () => {
			window.removeEventListener("pointermove", move);
			document.documentElement.removeEventListener("pointerleave", out);
		};
	}, [shelfRects, grid, hidden, stepAside, covered, editing]);

	useEffect(() => {
		if (!editing && !menu) return;
		const report = () => {
			if (menu && menuRef.current) {
				const r = menuRef.current.getBoundingClientRect();
				void invoke("set_menu_open", {
					open: true,
					rect: { x: Math.round(r.left), y: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) }
				}).catch(() => {});
				return;
			}
			const top = pickerRef.current?.getBoundingClientRect().top ?? window.innerHeight - 260;
			void invoke("set_menu_open", {
				open: true,
				rect: { x: 0, y: Math.round(top - 8), width: window.innerWidth, height: Math.round(window.innerHeight - top + 8) }
			}).catch(() => {});
		};
		const frame = requestAnimationFrame(report);
		// Said again: the dock's own menus closing clear it. (Once per change
		// of menu or editing, not on every render while dragging.)
		const timer = window.setInterval(report, 500);
		return () => {
			cancelAnimationFrame(frame);
			window.clearInterval(timer);
		};
	}, [editing, menu]);
	useEffect(() => {
		if (editing || menu) return;
		void invoke("set_menu_open", { open: false, rect: null }).catch(() => {});
	}, [editing, menu]);

	// A press elsewhere (Bloom's mouse hook says) closes the menu and editing.
	useEffect(() => {
		const off = listen("dock-dismiss-menus", () => {
			setMenu(null);
			setEditing(false);
		});
		const onKey = (e: KeyboardEvent) => {
			if (e.key !== "Escape") return;
			// A drag first; then the menu and editing.
			if (dragRef.current) {
				setDrag(null);
				return;
			}
			setMenu(null);
			setEditing(false);
		};
		window.addEventListener("keydown", onKey);
		return () => {
			void off.then((f) => f());
			window.removeEventListener("keydown", onKey);
		};
	}, []);

	// ---- dragging (from the picker, an item, or one widget out of a group)
	const dragRef = useRef<Drag | null>(null);
	dragRef.current = drag;
	const spanOf = (size: WidgetSize) => (size === "large" ? 4 : size === "wide" ? 2 : 1);
	const widthOf = (size: WidgetSize) => (grid ? spanOf(size) * grid.cell + (spanOf(size) - 1) * grid.gap : 0);

	/**
	 * Where a drag lands: over another widget, into its group; otherwise the
	 * grid spot nearest where the dragged widget itself is (its edges, not just
	 * the pointer), on the side its middle is on. Nothing well above the dock.
	 */
	const landing = (d: Drag): Landing | null => {
		if (!grid) return null;
		if (d.y < window.innerHeight - (grid.cell + 2 * grid.pad) - 40 * scale) return null;
		const left = d.x - d.grabX;
		const middle = left + widthOf(d.size) / 2;
		const side: Side = middle < window.innerWidth / 2 ? "left" : "right";
		const source = d.itemId && d.fromSide ? layout[d.fromSide].find((p) => p.id === d.itemId) : undefined;
		const over = layout[side].find((p) => {
			if (p.id === d.itemId) return false;
			const place = placeOf(grid, side, p);
			return d.x >= place.left && d.x <= place.left + place.width;
		});
		if (over) {
			const bringing = d.kind ? [d.kind] : (source?.widgets ?? []);
			return { side, group: over, ok: bringing.some((k) => !over.widgets.includes(k)) };
		}
		// A whole item moving doesn't stand in its own way; one pulled out of a
		// group leaves the group where it is.
		const others = layout[side].filter((p) => !(p.id === d.itemId && !d.kind));
		const cells = usable(side);
		const pitch = grid.cell + grid.gap;
		const spot = (size: WidgetSize): Landing | null => {
			// The spot nearest the widget's own left edge (kept where the pointer
			// holds it, so a smaller size stays under the pointer too).
			const at = d.x - Math.min(d.grabX, widthOf(size) - 8 * scale);
			let best = -1;
			let bestDistance = Infinity;
			for (let cell = 0; cell + spanOf(size) <= cells; cell++) {
				const distance = Math.abs(placeOf(grid, side, { cell, size }).left - at);
				if (distance < bestDistance) {
					best = cell;
					bestDistance = distance;
				}
			}
			if (best < 0) return null;
			if (fits(others, best, size, cells)) return { side, cell: best, size, ok: true };
			// Taken: a free spot a cell either way, if the widget is that close to it.
			for (const cell of [best - 1, best + 1]) {
				if (fits(others, cell, size, cells) && Math.abs(placeOf(grid, side, { cell, size }).left - at) < pitch) {
					return { side, cell, size, ok: true };
				}
			}
			return { side, cell: best, size, ok: false };
		};
		// A whole item keeps its size; a new one (from the picker, or out of a
		// group) shrinks to fit the gap it's dropped in.
		const sizes: WidgetSize[] = d.kind
			? (["large", "wide", "small"] as WidgetSize[]).filter((s) => spanOf(s) <= spanOf(d.size))
			: [d.size];
		let first: Landing | null = null;
		for (const size of sizes) {
			const land = spot(size);
			if (land?.ok) return land;
			first ??= land;
		}
		return first;
	};

	const startDrag = (e: React.PointerEvent, d: Omit<Drag, "startX" | "startY" | "x" | "y" | "moved">) => {
		if (e.button !== 0) return;
		e.preventDefault();
		e.stopPropagation();
		setMenu(null);
		setDrag({ ...d, startX: e.clientX, startY: e.clientY, x: e.clientX, y: e.clientY, moved: false });
	};

	useEffect(() => {
		if (!drag) return;
		const move = (e: PointerEvent) =>
			setDrag((d) =>
				d
					? {
							...d,
							x: e.clientX,
							y: e.clientY,
							// From where it started, so a slow drag counts too.
							moved: d.moved || Math.abs(e.clientX - d.startX) + Math.abs(e.clientY - d.startY) > 4
						}
					: d
			);
		const up = (e: PointerEvent) => {
			const d: Drag = { ...drag, x: e.clientX, y: e.clientY };
			setDrag(null);
			if (!drag.moved || !grid) return;
			const land = landing(d);
			if (!land || !land.ok) return;
			const next: Layout = { version: 2, left: [...layout.left], right: [...layout.right] };
			const source = d.itemId && d.fromSide ? next[d.fromSide].find((p) => p.id === d.itemId) : undefined;
			// What leaves where it was: one widget out of its group, or the item.
			const pull = () => {
				if (!source || !d.fromSide) return;
				if (d.kind) {
					const rest = source.widgets.filter((k) => k !== d.kind);
					next[d.fromSide] = next[d.fromSide].map((p) => (p.id === source.id ? { ...p, widgets: rest } : p));
					setShown((s) => ({ ...s, [source.id]: 0 }));
				} else next[d.fromSide] = next[d.fromSide].filter((p) => p.id !== source.id);
			};
			if (land.group) {
				const group = land.group;
				const bringing = d.kind ? [d.kind] : (source?.widgets ?? []);
				const merged = [...group.widgets, ...bringing.filter((k) => !group.widgets.includes(k))];
				pull();
				next[land.side] = next[land.side].map((p) => (p.id === group.id ? { ...p, widgets: merged } : p));
				// Show the first one just added.
				setShown((s) => ({ ...s, [group.id]: group.widgets.length }));
				save(next);
				return;
			}
			pull();
			const item: Placed =
				source && !d.kind
					? { ...source, cell: land.cell }
					: { id: newId(), cell: land.cell, size: land.size, widgets: [d.kind!], rotate: 0 };
			next[land.side] = [...next[land.side], item];
			save(next);
		};
		window.addEventListener("pointermove", move);
		window.addEventListener("pointerup", up, { once: true });
		return () => {
			window.removeEventListener("pointermove", move);
			window.removeEventListener("pointerup", up);
		};
	}, [drag, grid, layout]);

	// ---- changes from the menu
	const change = (side: Side, id: string, f: (p: Placed) => Placed | null) => {
		const next: Layout = { version: 2, left: layout.left, right: layout.right };
		next[side] = layout[side].map((p) => (p.id === id ? f(p) : p)).filter((p): p is Placed => p !== null);
		save(next);
	};
	const setSize = (side: Side, item: Placed, size: WidgetSize) => {
		if (!grid || !fits(layout[side], item.cell, size, usable(side), item.id)) return false;
		change(side, item.id, (p) => ({ ...p, size }));
		return true;
	};
	const takeOut = (side: Side, item: Placed, kind: WidgetKind) => {
		if (!grid) return;
		const rest = item.widgets.filter((k) => k !== kind);
		const others = layout[side].map((p) => (p.id === item.id ? { ...p, widgets: rest } : p));
		const size = widgetInfo(kind)?.size ?? "small";
		let cell = 0;
		while (cell < usable(side) && !fits(others, cell, size, usable(side))) cell += 1;
		if (cell >= usable(side)) return;
		save({ ...layout, [side]: [...others, { id: newId(), cell, size, widgets: [kind], rotate: 0 }] });
	};
	const setOption = (side: Side, item: Placed, kind: WidgetKind, key: string, value: string) =>
		change(side, item.id, (p) => ({
			...p,
			options: { ...(p.options ?? {}), [kind]: { ...(p.options?.[kind] ?? {}), [key]: value } }
		}));

	if (!grid) return null;
	const bottomBand = grid.cell + 2 * grid.pad;
	const menuItem = menu ? layout[menu.side].find((p) => p.id === menu.itemId) : undefined;
	const menuKind = menuItem ? menuItem.widgets[(shown[menuItem.id] ?? 0) % menuItem.widgets.length] : undefined;

	return (
		<div className={`dw-root${editing ? " is-editing" : ""}`}>
			{/* The grid, while editing */}
			{editing &&
				(["left", "right"] as Side[]).map((side) =>
Array.from({ length: usable(side) }, (_, cell) => {
						const place = placeOf(grid, side, { cell, size: "small" });
						return (
							<div
								key={`${side}${cell}`}
								className="dw-cell"
								style={{ left: place.left, width: grid.cell, height: grid.cell, bottom: grid.pad }}
							/>
						);
					})
				)}

{shelfRects.map((shelf) => (
				<div
					key={shelf.key}
					className={`dw-shelf${sideHidden(shelf.side) || shelf.vis !== "shown" ? " is-hidden" : ""}`}
					style={{ left: shelf.left, width: shelf.width, height: bottomBand, borderRadius: `${18 * scale}px ${18 * scale}px 0 0` }}
>
					<EdgeSurface edge="bottom" flares />
					{shelf.items.map((item) => {
						const place = placeOf(grid, shelf.side, item);
						const index = (shown[item.id] ?? 0) % item.widgets.length;
						const kind = item.widgets[index];
						// A whole item being moved; one widget pulled out leaves it be.
						const dragging = drag?.itemId === item.id && drag.moved && !drag.kind;
						return (
							<div
								key={item.id}
								className={`dw-item dw-item--${item.size}${dragging ? " is-dragging" : ""}`}
								style={{ left: place.left - shelf.left, width: place.width, height: grid.cell, bottom: grid.pad }}
								onPointerEnter={() => {
									hovered.current = item.id;
									if (editing) setOpenGroup(item.widgets.length > 1 ? item.id : null);
								}}
								onPointerLeave={() => (hovered.current = null)}
								onWheel={(e) => onWheel(item, e)}
								onPointerDown={(e) => {
									if (!editing) return;
									const r = e.currentTarget.getBoundingClientRect();
									startDrag(e, { itemId: item.id, fromSide: shelf.side, size: item.size, grabX: e.clientX - r.left, grabY: e.clientY - r.top });
								}}
								onContextMenu={(e) => {
									e.preventDefault();
									e.stopPropagation();
									setMenu({ side: shelf.side, itemId: item.id, x: e.clientX, y: e.clientY });
								}}
							>
								<div className="dw-face" key={kind}>
									<Widget kind={kind} size={item.size} id={item.id} options={item.options?.[kind]} data={data} />
								</div>
								{item.widgets.length > 1 && (
									<div className="dw-dots">
										{item.widgets.map((k, i) => (
											<span key={k} className={i === index ? "is-on" : ""} />
										))}
									</div>
								)}
								{editing && (
									<button
										type="button"
										className="dw-remove"
										title="Remove"
										onPointerDown={(e) => e.stopPropagation()}
										onClick={(e) => {
											e.stopPropagation();
											change(shelf.side, item.id, () => null);
										}}
									>
										<X size={10} strokeWidth={2.4} />
									</button>
								)}
							</div>
						);
					})}
				</div>
			))}

			{/* A group's widgets, while editing: drag one out (onto the grid or
			    another widget), click to show it, × to remove it */}
			{editing &&
				!drag?.moved &&
				shelfRects.flatMap((shelf) =>
					shelf.items
						.filter((item) => item.id === openGroup && item.widgets.length > 1)
						.map((item) => {
							const place = placeOf(grid, shelf.side, item);
							const index = (shown[item.id] ?? 0) % item.widgets.length;
							return (
								<div
									key={`m${item.id}`}
									className="dw-members"
									style={{
										left: Math.max(8, Math.min(window.innerWidth - 8, place.left + place.width / 2)),
										bottom: bottomBand + 6 * scale
									}}
								>
									{item.widgets.map((k, i) => (
										<div
											key={k}
											className={`dw-member${i === index ? " is-on" : ""}`}
											title="Drag out of the group"
											onPointerDown={(e) => {
												const size = widgetInfo(k)?.size ?? "small";
												startDrag(e, {
													kind: k,
													itemId: item.id,
													fromSide: shelf.side,
													size,
													grabX: widthOf(size) / 2,
													grabY: grid.cell / 2
												});
											}}
											onClick={() => setShown((s) => ({ ...s, [item.id]: i }))}
										>
											<span>{widgetInfo(k)?.label}</span>
											<button
												type="button"
												title="Remove"
												onPointerDown={(e) => e.stopPropagation()}
												onClick={(e) => {
													e.stopPropagation();
													const rest = item.widgets.filter((w) => w !== k);
													change(shelf.side, item.id, (p) => ({ ...p, widgets: rest }));
													setShown((s) => ({ ...s, [item.id]: 0 }));
												}}
											>
												<X size={9} strokeWidth={2.6} />
											</button>
										</div>
									))}
								</div>
							);
						})
				)}

			{/* Where a dragged widget would land */}
			{drag?.moved &&
				(() => {
					const land = landing(drag);
					if (!land) return null;
					const place = land.group
						? placeOf(grid, land.side, land.group)
: placeOf(grid, land.side, { cell: land.cell, size: land.size });
					return (
						<div
							className={`dw-drop${land.group ? " dw-drop--group" : ""}${land.ok ? "" : " is-blocked"}`}
							style={{ left: place.left, width: place.width, height: grid.cell, bottom: grid.pad }}
						>
							{land.group && <span>{land.ok ? "Add to group" : "Already there"}</span>}
						</div>
					);
				})()}

			{/* The widget itself, held where it was picked up */}
			{drag?.moved &&
				(() => {
					const source = drag.itemId && drag.fromSide ? layout[drag.fromSide].find((p) => p.id === drag.itemId) : undefined;
					const kind = drag.kind ?? (source ? source.widgets[(shown[source.id] ?? 0) % source.widgets.length] : undefined);
					if (!kind) return null;
					// The size it would land at (shrunk to fit a gap), held where it was picked up.
					const land = landing(drag);
					const size = land?.size && land.ok ? land.size : drag.size;
					const width = widthOf(size);
					return (
						<div
							className={`dw-ghost-card${land ? "" : " is-away"}`}
							style={{ left: drag.x - Math.min(drag.grabX, width - 8 * scale), top: drag.y - drag.grabY, width, height: grid.cell }}
						>
							<div className="dw-face">
								<Widget kind={kind} size={size} id={source?.id ?? "ghost"} options={source?.options?.[kind]} data={data} />
							</div>
							{source && !drag.kind && source.widgets.length > 1 && (
								<span className="dw-ghost-count">{source.widgets.length}</span>
							)}
						</div>
					);
				})()}

			{/* The picker */}
			{editing && (
				<div ref={pickerRef} className="dw-picker"style={{ bottom: bottomBand + 40 * scale }}>
					<div className="dw-picker-head">
						<Layers size={14} />
<span>Drag widgets beside the dock. Drop one on another to group them. Point at a group to list its widgets: drag one out, or × to remove it.</span>
						<button type="button" className="dw-done" onClick={() => setEditing(false)}>
							<Check size={13} /> Done
						</button>
					</div>
					<div className="dw-picker-sections">
						{(["Status", "Clock & calendar", "Now playing", "Tasks & timers"] as const).map((section) => (
							<div key={section} className="dw-picker-section">
								<span className="dw-picker-title">{section}</span>
								<div className="dw-picker-list">
									{WIDGETS.filter((w) => w.section === section).map((w) => (
										<button
											type="button"
											key={w.kind}
											className="dw-chip"
											onPointerDown={(e) => startDrag(e, { kind: w.kind, size: w.size, grabX: widthOf(w.size) / 2, grabY: grid.cell / 2 })}
										>
											<Plus size={11} /> {w.label}
										</button>
									))}
								</div>
							</div>
						))}
					</div>
				</div>
			)}

			{/* A widget's menu */}
			{menu && menuItem && menuKind && (
				<>
					<div className="dw-menu-backdrop" onPointerDown={() => setMenu(null)} />
					<div
						ref={menuRef}
						className="dw-menu"
						style={{ left: Math.min(menu.x, window.innerWidth - 240), bottom: window.innerHeight - menu.y + 6 }}
						onPointerDown={(e) => e.stopPropagation()}
					>
						<div className="dw-menu-title">
							{menuItem.widgets.length > 1
								? `Group: ${menuItem.widgets.map((k) => widgetInfo(k)?.label).join(", ")}`
								: widgetInfo(menuKind)?.label}
						</div>
						<div className="dw-menu-row">
							{(["small", "wide", "large"] as WidgetSize[]).map((size) => (
								<button
									type="button"
									key={size}
									className={`dw-seg${menuItem.size === size ? " is-on" : ""}`}
disabled={!fits(layout[menu.side], menuItem.cell, size, usable(menu.side), menuItem.id)}
									onClick={() => setSize(menu.side, menuItem, size)}
								>
									{size === "small" ? "Small" : size === "wide" ? "Wide" : "Large"}
								</button>
							))}
						</div>
						{menuKind === "worldclock" && (
							<select
								className="dw-select"
								value={menuItem.options?.worldclock?.zone ?? "America/New_York"}
								onChange={(e) => setOption(menu.side, menuItem, "worldclock", "zone", e.target.value)}
							>
								{ZONES.map((z) => (
									<option key={z.zone} value={z.zone}>
										{z.city}
									</option>
								))}
							</select>
						)}
						{menuKind === "countdown" && (
							<div className="dw-menu-form">
								<input
									className="dw-input"
									placeholder="What's coming"
									defaultValue={menuItem.options?.countdown?.label ?? ""}
									onBlur={(e) => setOption(menu.side, menuItem, "countdown", "label", e.target.value)}
								/>
								<input
									className="dw-input"
									type="date"
									defaultValue={menuItem.options?.countdown?.date ?? ""}
									onChange={(e) => setOption(menu.side, menuItem, "countdown", "date", e.target.value)}
								/>
							</div>
						)}
						{menuKind === "focus" && (
							<div className="dw-menu-row">
								{FOCUS_LENGTHS.map((min) => (
									<button
										type="button"
										key={min}
										className={`dw-seg${readFocus(menuItem.id).length === min ? " is-on" : ""}`}
										onClick={() => writeFocus(menuItem.id, { length: min, endsAt: null, left: min * 60 })}
									>
										{min} min
									</button>
								))}
							</div>
						)}
						{menuItem.widgets.length > 1 && (
							<>
								<div className="dw-menu-label">Turn on its own</div>
								<div className="dw-menu-row">
									{ROTATIONS.map((sec) => (
										<button
											type="button"
											key={sec}
											className={`dw-seg${menuItem.rotate === sec ? " is-on" : ""}`}
											onClick={() => change(menu.side, menuItem.id, (p) => ({ ...p, rotate: sec }))}
										>
											{sec === 0 ? "Off" : `${sec} s`}
										</button>
									))}
								</div>
								<button type="button" className="dw-menu-item" onClick={() => (takeOut(menu.side, menuItem, menuKind), setMenu(null))}>
									Take {widgetInfo(menuKind)?.label} out of the group
								</button>
							</>
						)}
						<div className="dw-menu-label">
							{menuItem.widgets.length > 1 ? "When it steps aside (the whole group)" : "When it steps aside"}
						</div>
						<div className="dw-menu-row">
							{DESKTOP_RULES.map(([rule, label, hint]) => (
								<button
									type="button"
									key={rule}
									title={hint}
									className={`dw-seg${(menuItem.desktop ?? "smart") === rule ? " is-on" : ""}`}
									onClick={() =>
										change(menu.side, menuItem.id, (p) => ({ ...p, desktop: rule === "smart" ? undefined : rule }))
									}
								>
									{label}
								</button>
							))}
						</div>
						{(menuItem.desktop ?? "smart") === "smart" && (
							<>
								<div className="dw-menu-label">Smart hides it on</div>
								<div className="dw-menu-row">
									{SMART_HIDES.map(([value, label, hint]) => (
										<button
											type="button"
											key={label}
											title={hint}
											className={`dw-seg${menuItem.smartHide === value ? " is-on" : ""}`}
											onClick={() => change(menu.side, menuItem.id, (p) => ({ ...p, smartHide: value }))}
										>
											{label}
										</button>
									))}
								</div>
								<div className="dw-menu-hint">Point at its spot to show it.</div>
							</>
						)}
						<button type="button" className="dw-menu-item" onClick={() => (setEditing(true), setMenu(null))}>
							Edit widgets
						</button>
						<button
							type="button"
							className="dw-menu-item dw-danger"
							onClick={() => {
								if (menuItem.widgets.length > 1) {
									const rest = menuItem.widgets.filter((k) => k !== menuKind);
									change(menu.side, menuItem.id, (p) => ({ ...p, widgets: rest }));
								} else change(menu.side, menuItem.id, () => null);
								setMenu(null);
							}}
						>
							Remove {widgetInfo(menuKind)?.label}
						</button>
					</div>
				</>
			)}
		</div>
	);
}

