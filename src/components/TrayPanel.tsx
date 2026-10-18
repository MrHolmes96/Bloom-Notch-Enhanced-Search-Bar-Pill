import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
	ChevronDown,
	ChevronLeft,
	ExternalLink,
	Eye,
	EyeOff,
	Pencil,
	RefreshCw
} from "lucide-react";
import { useStoredFlag, useStoredString } from "../settings/useStoredSetting";
import { DockIconImage } from "../dockIcon";
import {
	TRAY_FOLLOW_KEY,
	TRAY_PLACEMENT_KEY,
	iconLabels,
	parsePlacements,
	visibilityOf,
	withVisibility,
	type TrayIconInfo,
	type TrayState,
	type Visibility
} from "../trayLayout";
import "./QuickPanel.css";
import "./TrayPanel.css";

/**
 * The system tray inside the notch, opened from the control center's tray
 * button: one tile per tray icon, with the icon's live picture (as Windows'
 * tray shows it right now), updated the moment an app changes it. Click runs
 * an icon's default action, double click its double-click action, right-click
 * opens the app's own tray menu, as on the taskbar. The notch stays open while
 * the menu or flyout an icon opened is on screen.
 *
 * Hidden icons (hidden here, or by Tray Icon Manager) are in the "Hidden
 * icons" menu below, where they still work. The pencil switches to editing:
 * hide an icon, or show a hidden one again.
 */

const PER_ROW = 5;
const TILE = 64;
const GAP = 6;
const SECTION = 28;
/** The scrolling area shows at most about three rows of tiles. */
const MAX_BODY = 3 * TILE + 2 * GAP + SECTION + GAP;

function rowsHeight(count: number) {
	if (count === 0) return 0;
	const rows = Math.ceil(count / PER_ROW);
	return rows * TILE + (rows - 1) * GAP;
}

/** The panel's height inside the open notch, below its 36 px header. */
export function trayPanelHeight(shown: number, hidden: number, menuOpen: boolean) {
	let body = shown > 0 ? rowsHeight(shown) : 40;
	if (hidden > 0) body += GAP + SECTION + (menuOpen ? GAP + rowsHeight(hidden) : 0);
	return 4 + 26 + 6 + Math.min(body, MAX_BODY) + 12;
}

/** Program icons, for tray icons Bloom has no live picture of. */
const iconCache = new Map<string, string | null>();

function TrayTile({
	icon,
	label,
	visibility,
	editing,
	onChange,
	onError,
	onInteract
}: {
	icon: TrayIconInfo;
	label: string;
	visibility: Visibility;
	editing: boolean;
	onChange: (visibility: Visibility) => void;
	onError: (text: string) => void;
	/** Holds the notch open for this click; call the result if it failed. */
	onInteract: (id: string) => () => void;
}) {
	const [programIcon, setProgramIcon] = useState<string | null>(
		() => iconCache.get(icon.path) ?? null
	);

	useEffect(() => {
		if (icon.image || iconCache.has(icon.path)) return;
		void invoke<string | null>("get_app_icon", { path: icon.path })
			.then((value) => {
				iconCache.set(icon.path, value ?? null);
				setProgramIcon(value ?? null);
			})
			.catch(() => iconCache.set(icon.path, null));
	}, [icon.path, icon.image]);
	const image = icon.image ?? programIcon;

	const title = editing
		? label
		: `${label}${icon.tooltip && icon.tooltip !== label ? ` (${icon.tooltip})` : ""}: click to open, right-click for its menu`;

	return (
		<div className={`tray-tile-wrap${visibility === "hidden" ? " is-hidden" : ""}`}>
			<button
				type="button"
				className="tray-tile"
				title={title}
				onClick={(e) => {
					e.stopPropagation();
					if (editing) {
						if (e.detail <= 1) onChange(visibility === "hidden" ? "shown" : "hidden");
						return;
					}
					const failed = onInteract(icon.id);
					// The second click of a double click is the icon's double
					// click, as on the taskbar.
					const command = e.detail >= 2 ? "double_click_tray_icon" : "activate_tray_icon";
					invoke(command, { trayId: icon.id }).catch((err) => {
						failed();
						onError(String(err));
					});
				}}
				onContextMenu={(e) => {
					e.preventDefault();
					e.stopPropagation();
					if (editing) return;
					const failed = onInteract(icon.id);
					invoke("show_tray_context_menu", { trayId: icon.id }).catch((err) => {
						failed();
						onError(String(err));
					});
				}}
			>
				{image ? (
					<DockIconImage className="tray-tile-icon" src={image} alt="" />
				) : (
					<span className="tray-tile-icon tray-tile-letter">{label[0]}</span>
				)}
				<span className="tray-tile-name">{label}</span>
			</button>
			{editing && (
				<div className="tray-tile-edit">
					{visibility === "hidden" ? (
						<button type="button" title="Show in the tray" onClick={() => onChange("shown")}>
							<Eye />
						</button>
					) : (
						<button type="button" title="Hide" onClick={() => onChange("hidden")}>
							<EyeOff />
						</button>
					)}
				</div>
			)}
		</div>
	);
}

export function TrayPanel({
	onBack,
	onHeightChange,
	onPinChange
}: {
	onBack: () => void;
	onHeightChange: (height: number) => void;
	/** Keeps the notch open while an icon's menu or flyout is on screen. */
	onPinChange?: (pinned: boolean) => void;
}) {
	const [state, setState] = useState<TrayState | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [editing, setEditing] = useState(false);
	const [menuOpen, setMenuOpen] = useState(false);
	const [placementsRaw, setPlacementsRaw] = useStoredString(TRAY_PLACEMENT_KEY, "{}");
	const [followManager] = useStoredFlag(TRAY_FOLLOW_KEY, true);
	const placements = useMemo(() => parsePlacements(placementsRaw), [placementsRaw]);

	// One load at a time; a change while loading loads once more after.
	const loadingRef = useRef(false);
	const againRef = useRef(false);
	const load = async (manual = false) => {
		if (loadingRef.current) {
			againRef.current = true;
			return;
		}
		loadingRef.current = true;
		if (manual) setLoading(true);
		try {
			do {
				againRef.current = false;
				setState(await invoke<TrayState>("get_tray_state"));
			} while (againRef.current);
			setError(null);
		} catch (e) {
			setError(String(e));
		} finally {
			loadingRef.current = false;
			setLoading(false);
		}
	};

	useEffect(() => {
		void load();
		// Icons appear, change and leave live, as in Windows' tray.
		const changed = listen("tray-changed", () => void load());
		return () => {
			void changed.then((f) => f());
		};
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, []);

	// A message about a failed click clears itself.
	useEffect(() => {
		if (!error) return;
		const timer = window.setTimeout(() => setError(null), 4000);
		return () => window.clearTimeout(timer);
	}, [error]);

	// While an icon's menu or flyout is open the notch stays open, however
	// long that is, so it is still there when the menu closes. Bloom says when
	// what each click opened is gone again; a click that failed lets go at
	// once. Only if Bloom never answers does the notch let go, after 10 min.
	const openRef = useRef(new Map<string, number>());
	const backstopRef = useRef<number | null>(null);
	const pinChangeRef = useRef(onPinChange);
	pinChangeRef.current = onPinChange;
	const releaseAll = () => {
		if (backstopRef.current !== null) window.clearTimeout(backstopRef.current);
		backstopRef.current = null;
		if (openRef.current.size > 0) {
			openRef.current.clear();
			pinChangeRef.current?.(false);
		}
	};
	const release = (id: string) => {
		const open = openRef.current;
		const count = open.get(id) ?? 0;
		if (count <= 0) return;
		if (count > 1) open.set(id, count - 1);
		else open.delete(id);
		if (open.size === 0) releaseAll();
	};
	const pin = (id: string) => {
		const open = openRef.current;
		if (open.size === 0) pinChangeRef.current?.(true);
		open.set(id, (open.get(id) ?? 0) + 1);
		if (backstopRef.current !== null) window.clearTimeout(backstopRef.current);
		backstopRef.current = window.setTimeout(releaseAll, 10 * 60 * 1000);
		let done = false;
		return () => {
			if (done) return;
			done = true;
			release(id);
		};
	};
	useEffect(() => {
		const ended = listen<string>("tray-interaction-end", (event) => release(event.payload));
		return () => {
			void ended.then((f) => f());
			releaseAll();
		};
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, []);

	const icons = useMemo(() => state?.icons ?? [], [state]);
	const labels = useMemo(() => iconLabels(icons), [icons]);
	const groups = useMemo(() => {
		const result: Record<Visibility, TrayIconInfo[]> = { shown: [], hidden: [] };
		for (const icon of icons) result[visibilityOf(icon, placements, followManager)].push(icon);
		return result;
	}, [icons, placements, followManager]);
	const hiddenOpen = menuOpen || editing;

	useEffect(() => {
		onHeightChange(trayPanelHeight(groups.shown.length, groups.hidden.length, hiddenOpen));
	}, [groups, hiddenOpen, onHeightChange]);

	const change = (icon: TrayIconInfo, visibility: Visibility) =>
		setPlacementsRaw(JSON.stringify(withVisibility(placements, icon, visibility, followManager)));

	const tiles = (list: TrayIconInfo[], visibility: Visibility) => (
		<div className="tray-grid">
			{list.map((icon) => (
				<TrayTile
					key={icon.id}
					icon={icon}
					label={labels.get(icon.id) ?? icon.name}
					visibility={visibility}
					editing={editing}
					onChange={(next) => change(icon, next)}
					onError={setError}
					onInteract={pin}
				/>
			))}
		</div>
	);

	return (
		<div className="qp" onClick={(e) => e.stopPropagation()}>
			<div className="qp-header">
				<button className="qp-icon-btn" onClick={onBack} aria-label="Back to controls">
					<ChevronLeft />
				</button>
				<span className="qp-title">{editing ? "Hide or Show Icons" : "System Tray"}</span>
				<button
					className={`qp-icon-btn${editing ? " is-active" : ""}`}
					onClick={() => setEditing((e) => !e)}
					aria-label="Hide or show icons"
					title={editing ? "Done" : "Hide or show icons"}
				>
					<Pencil />
				</button>
				<button
					className="qp-icon-btn"
					onClick={() => void load(true)}
					disabled={loading}
					aria-label="Refresh"
					title="Refresh"
				>
					<RefreshCw className={loading ? "qp-spin" : ""} />
				</button>
				<button
					className="qp-icon-btn"
					onClick={() => void invoke("open_system_tray")}
					aria-label="Open Windows' tray"
					title="Open Windows' own tray"
				>
					<ExternalLink />
				</button>
			</div>
			{error && <div className="tray-message">{error}</div>}
			{!state && !error && <div className="tray-message">Loading the tray…</div>}
			{state && (
				<div className="tray-body" style={{ maxHeight: MAX_BODY }}>
					{groups.shown.length > 0
						? tiles(groups.shown, "shown")
						: !error && (
								<div className="tray-message">
									{icons.length === 0
										? "No apps are in the tray right now"
										: "Every tray icon is hidden"}
								</div>
							)}
					{groups.hidden.length > 0 && (
						<>
							<button
								type="button"
								className={`tray-section${hiddenOpen ? " is-open" : ""}`}
								onClick={() => setMenuOpen((open) => !open)}
								disabled={editing}
							>
								<span>Hidden icons</span>
								<span className="tray-section-count">{groups.hidden.length}</span>
								<ChevronDown className="tray-section-chevron" />
							</button>
							{hiddenOpen && tiles(groups.hidden, "hidden")}
						</>
					)}
				</div>
			)}
		</div>
	);
}
