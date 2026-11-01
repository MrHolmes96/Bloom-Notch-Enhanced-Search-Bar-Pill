import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ChevronRight } from "lucide-react";
import {
	DisplayIcon,
	FileIcon,
	FolderIcon,
	MoreIcon,
	NewIcon,
	PersonaliseIcon,
	RefreshIcon,
	SortIcon,
	TerminalIcon,
	ViewIcon
} from "./DesktopMenuIcons";
import "./DesktopMenu.css";

/**
 * The desktop's right-click menu beside the dock, as Windows 11 shows it,
 * opened at the pointer (see `desktop_menu.rs`: Windows' own opens far above
 * it there). Each item does what Windows' does, through Bloom.
 */

interface ViewState {
	size: "large" | "medium" | "small";
	auto_arrange: boolean;
	grid: boolean;
	icons: boolean;
	sort: string;
}

interface NewItem {
	kind: string;
	name: string;
	/** Windows' own small icon for the type (a PNG data URL). */
	icon: string | null;
}

interface Payload {
	x: number;
	y: number;
	view: ViewState;
	new: NewItem[];
	terminal: boolean;
}

type Sub = "view" | "sort" | "new" | null;

interface Rect {
	x: number;
	y: number;
	width: number;
	height: number;
}

const GAP = 6;

/** Whether the desktop's menu is open (the dock leaves the click area to it). */
let openNow = false;
export const isDesktopMenuOpen = () => openNow;

export function DesktopMenu() {
	const [menu, setMenu] = useState<Payload | null>(null);
	const [place, setPlace] = useState<{ left: number; top: number } | null>(null);
	const [sub, setSub] = useState<Sub>(null);
	const [subPlace, setSubPlace] = useState<{ left: number; top: number } | null>(null);
	const menuRef = useRef<HTMLDivElement | null>(null);
	const subRef = useRef<HTMLDivElement | null>(null);
	const anchors = useRef<Record<string, HTMLDivElement | null>>({});
	const hoverTimer = useRef<number | null>(null);

	const close = () => {
		openNow = false;
		setMenu(null);
		setPlace(null);
		setSub(null);
		setSubPlace(null);
		invoke("set_menu_open", { open: false, rect: null }).catch(() => {});
	};

	useEffect(() => {
		const opened = listen<Payload>("desktop-menu", (e) => {
			setSub(null);
			setSubPlace(null);
			setPlace(null);
			openNow = true;
			setMenu(e.payload);
		});
		// A press anywhere else (Bloom's mouse hook says) closes it, as a
		// click outside any menu does.
		const dismissed = listen("dock-dismiss-menus", () => close());
		const onKey = (e: KeyboardEvent) => {
			if (e.key === "Escape") close();
		};
		window.addEventListener("keydown", onKey);
		return () => {
			void opened.then((f) => f());
			void dismissed.then((f) => f());
			window.removeEventListener("keydown", onKey);
		};
	}, []);

	// Above the pointer (Windows does the same at the bottom of the screen),
	// kept on the screen.
	useLayoutEffect(() => {
		if (!menu || !menuRef.current) return;
		const r = menuRef.current.getBoundingClientRect();
		setPlace({
			left: Math.max(GAP, Math.min(menu.x, window.innerWidth - r.width - GAP)),
			top: Math.max(GAP, Math.min(menu.y - r.height, window.innerHeight - r.height - GAP))
		});
	}, [menu]);

	// A sub-menu opens beside its item, on whichever side there's room.
	useLayoutEffect(() => {
		if (!sub || !subRef.current || !menuRef.current) return;
		const anchor = anchors.current[sub];
		if (!anchor) return;
		const a = anchor.getBoundingClientRect();
		const m = menuRef.current.getBoundingClientRect();
		const s = subRef.current.getBoundingClientRect();
		const right = m.right - 2;
		const left = right + s.width > window.innerWidth - GAP ? m.left - s.width + 2 : right;
		const top = Math.max(GAP, Math.min(a.top - 5, window.innerHeight - s.height - GAP));
		setSubPlace({ left, top });
	}, [sub, menu]);

	// The menu (and its open sub-menu) take clicks; the rest of the dock's
	// window lets them through.
	useEffect(() => {
		if (!menu || !place || !menuRef.current) return;
		const rects: Rect[] = [menuRef.current.getBoundingClientRect()];
		if (subPlace && subRef.current) rects.push(subRef.current.getBoundingClientRect());
		const x = Math.min(...rects.map((r) => r.x));
		const y = Math.min(...rects.map((r) => r.y));
		const right = Math.max(...rects.map((r) => r.x + r.width));
		const bottom = Math.max(...rects.map((r) => r.y + r.height));
		invoke("set_menu_open", {
			open: true,
			rect: {
				x: Math.round(x),
				y: Math.round(y),
				width: Math.round(right - x),
				height: Math.round(bottom - y)
			}
		}).catch(() => {});
	}, [menu, place, sub, subPlace]);

	if (!menu) return null;

	const run = (action: string) => {
		close();
		invoke("desktop_menu_run", { action }).catch(() => {});
	};

	const openSub = (which: Sub, now = false) => {
		if (hoverTimer.current !== null) window.clearTimeout(hoverTimer.current);
		hoverTimer.current = null;
		if (now) {
			setSubPlace(null);
			setSub(which);
			return;
		}
		hoverTimer.current = window.setTimeout(() => {
			setSubPlace(null);
			setSub(which);
		}, 220);
	};

	const item = (
		key: string,
		label: string,
		icon: React.ReactNode,
		action: string | null,
		submenu: Sub = null
	) => (
		<div
			key={key}
			ref={(el) => {
				if (submenu) anchors.current[submenu] = el;
			}}
			className={`dm-item${submenu && sub === submenu ? " is-open" : ""}`}
			role="menuitem"
			onPointerEnter={() => openSub(submenu)}
			onClick={(e) => {
				e.stopPropagation();
				if (submenu) openSub(submenu, true);
				else if (action) run(action);
			}}
		>
			<span className="dm-icon">{icon}</span>
			<span className="dm-label">{label}</span>
			{submenu && <ChevronRight className="dm-chevron" />}
		</div>
	);

	const choice = (key: string, label: string, on: boolean, kind: "radio" | "check", action: string) => (
		<div
			key={key}
			className="dm-item"
			role={kind === "radio" ? "menuitemradio" : "menuitemcheckbox"}
			aria-checked={on}
			onClick={(e) => {
				e.stopPropagation();
				run(action);
			}}
		>
			<span className="dm-icon dm-mark">
				{on && (kind === "radio" ? <span className="dm-dot" /> : <span className="dm-check">✓</span>)}
			</span>
			<span className="dm-label">{label}</span>
		</div>
	);

	const v = menu.view;
	const submenu =
		sub === "view" ? (
			<>
				{choice("l", "Large icons", v.size === "large", "radio", "view:large")}
				{choice("m", "Medium icons", v.size === "medium", "radio", "view:medium")}
				{choice("s", "Small icons", v.size === "small", "radio", "view:small")}
				<div className="dm-sep" />
				{choice("a", "Auto arrange icons", v.auto_arrange, "check", "view:auto")}
				{choice("g", "Align icons to grid", v.grid, "check", "view:grid")}
				<div className="dm-sep" />
				{choice("i", "Show desktop icons", v.icons, "check", "view:icons")}
			</>
		) : sub === "sort" ? (
			<>
				{choice("n", "Name", v.sort === "name", "radio", "sort:name")}
				{choice("z", "Size", v.sort === "size", "radio", "sort:size")}
				{choice("t", "Item type", v.sort === "type", "radio", "sort:type")}
				{choice("d", "Date modified", v.sort === "date", "radio", "sort:date")}
			</>
		) : sub === "new" ? (
			<>
				{menu.new.map((n, i) => (
					<div key={n.kind}>
						{item(
							n.kind,
							n.name,
							n.icon ? (
								<img className="dm-type-icon" src={n.icon} alt="" />
							) : n.kind.toLowerCase() === "folder" ? (
								<FolderIcon />
							) : (
								<FileIcon />
							),
							`new:${i}`
						)}
						{n.kind.toLowerCase() === ".lnk" && i < menu.new.length - 1 && <div className="dm-sep" />}
					</div>
				))}
			</>
		) : null;

	return (
		<>
			<div
				ref={menuRef}
				className="dm-menu"
				role="menu"
				style={{
					left: place?.left ?? menu.x,
					top: place?.top ?? menu.y,
					visibility: place ? "visible" : "hidden"
				}}
				onClick={(e) => e.stopPropagation()}
				onPointerLeave={() => {
					if (hoverTimer.current !== null) window.clearTimeout(hoverTimer.current);
				}}
			>
				{item("view", "View", <ViewIcon />, null, "view")}
				{item("sort", "Sort by", <SortIcon />, null, "sort")}
				{item("refresh", "Refresh", <RefreshIcon />, "refresh")}
				<div className="dm-sep" />
				{item("new", "New", <NewIcon />, null, "new")}
				<div className="dm-sep" />
				{item("display", "Display settings", <DisplayIcon />, "display")}
				{item("personalise", "Personalise", <PersonaliseIcon />, "personalise")}
				{menu.terminal && (
					<>
						<div className="dm-sep" />
						{item("terminal", "Open in Terminal", <TerminalIcon />, "terminal")}
					</>
				)}
				<div className="dm-sep" />
				{item("more", "Show more options", <MoreIcon />, "more")}
			</div>
			{submenu && (
				<div
					ref={subRef}
					className="dm-menu dm-sub"
					role="menu"
					style={{
						left: subPlace?.left ?? 0,
						top: subPlace?.top ?? 0,
						visibility: subPlace ? "visible" : "hidden"
					}}
					onClick={(e) => e.stopPropagation()}
					onPointerEnter={() => {
						if (hoverTimer.current !== null) window.clearTimeout(hoverTimer.current);
					}}
				>
					{submenu}
				</div>
			)}
		</>
	);
}
