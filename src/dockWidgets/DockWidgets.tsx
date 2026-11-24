import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
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
	WIDGETS,
	fits,
	gridFor,
	newId,
	placeOf,
	readLayout,
	shelves,
	widgetInfo,
	type Grid,
	type Layout,
	type Placed,
	type Side,
	type WidgetKind,
	type WidgetSize
} from "./model";
import { Widget, ZONES, readFocus, writeFocus, type WidgetData } from "./widgets";
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
}

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

export function DockWidgets({ dockRef, scale, hidden, stepAside }: Props) {
	const [layout, setLayout] = useState<Layout>(() => readLayout(localStorage.getItem(LAYOUT_KEY)));
	const [editing, setEditing] = useState(false);
	const [covered, setCovered] = useState<[boolean, boolean]>([false, false]);
	const [dock, setDock] = useState<{ left: number; right: number; height: number } | null>(null);
	const [width, setWidth] = useState(window.innerWidth);
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

	// ---- where the dock is: the grid hangs off it
	useLayoutEffect(() => {
		const measure = () => {
			const el = dockRef.current;
			setWidth(window.innerWidth);
			if (!el) return;
			const r = el.getBoundingClientRect();
			if (r.width < 40 || r.height < 30) return;
			setDock((prev) =>
				prev && Math.abs(prev.left - r.left) < 0.5 && Math.abs(prev.right - r.right) < 0.5 && Math.abs(prev.height - r.height) < 0.5
					? prev
					: { left: r.left, right: r.right, height: r.height }
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
		};
	}, [dockRef]);
	const grid: Grid | null = useMemo(() => (dock ? gridFor(dock, width, scale) : null), [dock, width, scale]);

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
	const data: WidgetData = {
		metrics,
		weather: {
			temperature: weather.temperature,
			condition: weather.weatherCondition,
			icon: weather.weatherIcon,
			city: weather.cityName,
			unit: weather.tempUnit === "fahrenheit" ? "F" : "C"
		},
		media,
		today,
		now,
		h24
	};

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
	const shelfRects = useMemo(() => {
		if (!grid) return [] as { side: Side; items: Placed[]; left: number; width: number }[];
		const out: { side: Side; items: Placed[]; left: number; width: number }[] = [];
		for (const side of ["left", "right"] as Side[]) {
			const items = layout[side].filter((p) => p.cell + (p.size === "large" ? 4 : p.size === "wide" ? 2 : 1) <= grid.cells[side]);
			for (const run of shelves(items)) {
				const places = run.map((p) => placeOf(grid, side, p));
				const left = Math.min(...places.map((p) => p.left)) - grid.pad;
				const right = Math.max(...places.map((p) => p.left + p.width)) + grid.pad;
				out.push({ side, items: run, left, width: right - left });
			}
		}
		return out;
	}, [grid, layout]);

	useEffect(() => {
		if (!dock) return;
		const height = dock.height;
		const rects = shelfRects
			.filter((s) => !sideHidden(s.side))
			.map((s) => ({
				x: Math.round(s.left),
				y: Math.round(window.innerHeight - height),
				width: Math.round(s.width),
				height: Math.round(height)
			}));
		void invoke("set_dock_widget_rects", { rects }).catch(() => {});
	}, [shelfRects, dock, hidden, stepAside, covered, editing]);

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
		// Said again: the dock's own menus closing clear it.
		const timer = window.setInterval(report, 500);
		return () => {
			cancelAnimationFrame(frame);
			window.clearInterval(timer);
		};
	});
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
		if (!grid || !dock) return null;
		if (d.y < window.innerHeight - dock.height - 40 * scale) return null;
		const left = d.x - d.grabX;
		const middle = left + widthOf(d.size) / 2;
		const side: Side = middle < (dock.left + dock.right) / 2 ? "left" : "right";
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
		const cells = grid.cells[side];
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
			const next: Layout = { version: 1, left: [...layout.left], right: [...layout.right] };
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
		const next: Layout = { version: 1, left: layout.left, right: layout.right };
		next[side] = layout[side].map((p) => (p.id === id ? f(p) : p)).filter((p): p is Placed => p !== null);
		save(next);
	};
	const setSize = (side: Side, item: Placed, size: WidgetSize) => {
		if (!grid || !fits(layout[side], item.cell, size, grid.cells[side], item.id)) return false;
		change(side, item.id, (p) => ({ ...p, size }));
		return true;
	};
	const takeOut = (side: Side, item: Placed, kind: WidgetKind) => {
		if (!grid) return;
		const rest = item.widgets.filter((k) => k !== kind);
		const others = layout[side].map((p) => (p.id === item.id ? { ...p, widgets: rest } : p));
		const size = widgetInfo(kind)?.size ?? "small";
		let cell = 0;
		while (cell < grid.cells[side] && !fits(others, cell, size, grid.cells[side])) cell += 1;
		if (cell >= grid.cells[side]) return;
		save({ ...layout, [side]: [...others, { id: newId(), cell, size, widgets: [kind], rotate: 0 }] });
	};
	const setOption = (side: Side, item: Placed, kind: WidgetKind, key: string, value: string) =>
		change(side, item.id, (p) => ({
			...p,
			options: { ...(p.options ?? {}), [kind]: { ...(p.options?.[kind] ?? {}), [key]: value } }
		}));

	if (!grid || !dock) return null;
	const bottomBand = dock.height;
	const menuItem = menu ? layout[menu.side].find((p) => p.id === menu.itemId) : undefined;
	const menuKind = menuItem ? menuItem.widgets[(shown[menuItem.id] ?? 0) % menuItem.widgets.length] : undefined;

	return (
		<div className={`dw-root${editing ? " is-editing" : ""}`}>
			{/* The grid, while editing */}
			{editing &&
				(["left", "right"] as Side[]).map((side) =>
					Array.from({ length: grid.cells[side] }, (_, cell) => {
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
					key={shelf.items.map((p) => p.id).join("-")}
					className={`dw-shelf${sideHidden(shelf.side) ? " is-hidden" : ""}`}
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
						<div className="dw-menu-title">{widgetInfo(menuKind)?.label}</div>
						<div className="dw-menu-row">
							{(["small", "wide", "large"] as WidgetSize[]).map((size) => (
								<button
									type="button"
									key={size}
									className={`dw-seg${menuItem.size === size ? " is-on" : ""}`}
									disabled={!fits(layout[menu.side], menuItem.cell, size, grid.cells[menu.side], menuItem.id)}
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

