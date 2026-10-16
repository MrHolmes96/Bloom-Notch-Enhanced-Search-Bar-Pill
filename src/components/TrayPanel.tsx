import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
	ChevronDown,
	ChevronLeft,
	ChevronUp,
	ExternalLink,
	Eye,
	EyeOff,
	Pencil,
	RefreshCw
} from "lucide-react";
import { useStoredFlag, useStoredString } from "../settings/useStoredSetting";
import {
	TRAY_FOLLOW_KEY,
	TRAY_PLACEMENT_KEY,
	iconLabels,
	parsePlacements,
	placementOf,
	withPlacement,
	type Placement,
	type TrayIconInfo,
	type TrayState
} from "../trayLayout";
import "./QuickPanel.css";
import "./TrayPanel.css";

/**
 * The system tray inside the notch, opened from the control center's tray
 * button. Icons on Windows' taskbar show first; the ones Windows keeps in its
 * hidden icons are one click below, as behind Windows' ^ button. Click runs an
 * icon's default action, right-click opens the app's own tray menu.
 *
 * The pencil switches to editing: move icons between the tray and the hidden
 * icons, or hide them from Bloom's tray altogether (and show them again).
 * Tray Icon Manager's hidden icons stay hidden here too when Settings > Notch
 * > System Tray says so.
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
export function trayPanelHeight(
	tray: number,
	menu: number,
	hidden: number,
	menuOpen: boolean,
	editing: boolean
) {
	let body = tray > 0 ? rowsHeight(tray) : 40;
	if (menu > 0) body += GAP + SECTION + (menuOpen ? GAP + rowsHeight(menu) : 0);
	if (editing && hidden > 0) body += GAP + SECTION + GAP + rowsHeight(hidden);
	return 4 + 26 + 6 + Math.min(body, MAX_BODY) + 12;
}

const iconCache = new Map<string, string | null>();

function TrayTile({
	icon,
	label,
	place,
	editing,
	onMove,
	onError
}: {
	icon: TrayIconInfo;
	label: string;
	place: Placement;
	editing: boolean;
	onMove: (place: Placement) => void;
	onError: (text: string) => void;
}) {
	const [image, setImage] = useState<string | null>(() => iconCache.get(icon.path) ?? null);

	useEffect(() => {
		if (iconCache.has(icon.path)) return;
		void invoke<string | null>("get_app_icon", { path: icon.path })
			.then((value) => {
				iconCache.set(icon.path, value ?? null);
				setImage(value ?? null);
			})
			.catch(() => iconCache.set(icon.path, null));
	}, [icon.path]);

	const title = editing
		? label
		: `${label}${icon.tooltip && icon.tooltip !== label ? ` (${icon.tooltip})` : ""}: click to open, right-click for its menu`;

	return (
		<div className={`tray-tile-wrap${place === "hidden" ? " is-hidden" : ""}`}>
			<button
				type="button"
				className="tray-tile"
				title={title}
				onClick={(e) => {
					e.stopPropagation();
					if (editing) return;
					invoke("activate_tray_icon", { trayId: icon.id }).catch((err) => onError(String(err)));
				}}
				onContextMenu={(e) => {
					e.preventDefault();
					e.stopPropagation();
					if (editing) return;
					invoke("show_tray_context_menu", { trayId: icon.id }).catch((err) =>
						onError(String(err))
					);
				}}
			>
				{image ? (
					<img className="tray-tile-icon" src={image} alt="" draggable={false} />
				) : (
					<span className="tray-tile-icon tray-tile-letter">{label[0]}</span>
				)}
				<span className="tray-tile-name">{label}</span>
			</button>
			{editing && (
				<div className="tray-tile-edit">
					{place === "tray" && (
						<button type="button" title="Move to hidden icons" onClick={() => onMove("menu")}>
							<ChevronDown />
						</button>
					)}
					{place === "menu" && (
						<button type="button" title="Move to the tray" onClick={() => onMove("tray")}>
							<ChevronUp />
						</button>
					)}
					{place === "hidden" ? (
						<button type="button" title="Show again" onClick={() => onMove("menu")}>
							<Eye />
						</button>
					) : (
						<button type="button" title="Hide from Bloom's tray" onClick={() => onMove("hidden")}>
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
	onHeightChange
}: {
	onBack: () => void;
	onHeightChange: (height: number) => void;
}) {
	const [state, setState] = useState<TrayState | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [editing, setEditing] = useState(false);
	const [menuOpen, setMenuOpen] = useState(() => {
		try {
			return localStorage.getItem("bloom-tray-menu-open") === "true";
		} catch {
			return false;
		}
	});
	const [placementsRaw, setPlacementsRaw] = useStoredString(TRAY_PLACEMENT_KEY, "{}");
	const [followManager] = useStoredFlag(TRAY_FOLLOW_KEY, true);
	const placements = useMemo(() => parsePlacements(placementsRaw), [placementsRaw]);

	const load = async () => {
		setLoading(true);
		try {
			setState(await invoke<TrayState>("get_tray_state"));
			setError(null);
		} catch (e) {
			setError(String(e));
		} finally {
			setLoading(false);
		}
	};

	useEffect(() => {
		void load();
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, []);

	const icons = state?.icons ?? [];
	const labels = useMemo(() => iconLabels(icons), [icons]);
	const groups = useMemo(() => {
		const result: Record<Placement, TrayIconInfo[]> = { tray: [], menu: [], hidden: [] };
		for (const icon of icons) result[placementOf(icon, placements, followManager)].push(icon);
		return result;
	}, [icons, placements, followManager]);

	useEffect(() => {
		onHeightChange(
			trayPanelHeight(
				groups.tray.length,
				groups.menu.length,
				groups.hidden.length,
				menuOpen,
				editing
			)
		);
	}, [groups, menuOpen, editing, onHeightChange]);

	const move = (icon: TrayIconInfo, place: Placement) => {
		// Showing a hidden icon again puts it back where Windows has it.
		const current = placementOf(icon, placements, followManager);
		let target = place;
		if (current === "hidden" && place !== "hidden") {
			const windowsPlace: Placement = icon.promoted ? "tray" : "menu";
			target = windowsPlace;
		}
		setPlacementsRaw(JSON.stringify(withPlacement(placements, icon, target, followManager)));
	};

	const toggleMenu = () => {
		const next = !menuOpen;
		setMenuOpen(next);
		try {
			localStorage.setItem("bloom-tray-menu-open", String(next));
		} catch {
			/* only remembered for convenience */
		}
	};

	const tiles = (list: TrayIconInfo[], place: Placement) => (
		<div className="tray-grid">
			{list.map((icon) => (
				<TrayTile
					key={icon.id}
					icon={icon}
					label={labels.get(icon.id) ?? icon.name}
					place={place}
					editing={editing}
					onMove={(next) => move(icon, next)}
					onError={setError}
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
				<span className="qp-title">{editing ? "Edit Tray" : "System Tray"}</span>
				<button
					className={`qp-icon-btn${editing ? " is-active" : ""}`}
					onClick={() => setEditing((e) => !e)}
					aria-label="Edit tray"
					title={editing ? "Done" : "Hide icons, or move them between the tray and hidden icons"}
				>
					<Pencil />
				</button>
				<button
					className="qp-icon-btn"
					onClick={() => void load()}
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
			{state && (
				<div className="tray-body" style={{ maxHeight: MAX_BODY }}>
					{groups.tray.length > 0
						? tiles(groups.tray, "tray")
						: !error && (
								<div className="tray-message">
									{icons.length === 0
										? "No apps are in the tray right now"
										: "Every icon is in hidden icons"}
								</div>
							)}
					{groups.menu.length > 0 && (
						<>
							<button
								type="button"
								className={`tray-section${menuOpen ? " is-open" : ""}`}
								onClick={toggleMenu}
							>
								<span>Hidden icons</span>
								<span className="tray-section-count">{groups.menu.length}</span>
								<ChevronDown className="tray-section-chevron" />
							</button>
							{menuOpen && tiles(groups.menu, "menu")}
						</>
					)}
					{editing && groups.hidden.length > 0 && (
						<>
							<div className="tray-section is-static">
								<span>Not shown in Bloom</span>
								<span className="tray-section-count">{groups.hidden.length}</span>
							</div>
							{tiles(groups.hidden, "hidden")}
						</>
					)}
				</div>
			)}
		</div>
	);
}
