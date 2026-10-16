import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
 * button: one tile per tray icon. Click runs an icon's default action,
 * right-click opens the app's own tray menu, as on the taskbar.
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

const iconCache = new Map<string, string | null>();

function TrayTile({
	icon,
	label,
	visibility,
	editing,
	onChange,
	onError
}: {
	icon: TrayIconInfo;
	label: string;
	visibility: Visibility;
	editing: boolean;
	onChange: (visibility: Visibility) => void;
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
		<div className={`tray-tile-wrap${visibility === "hidden" ? " is-hidden" : ""}`}>
			<button
				type="button"
				className="tray-tile"
				title={title}
				onClick={(e) => {
					e.stopPropagation();
					if (editing) {
						onChange(visibility === "hidden" ? "shown" : "hidden");
						return;
					}
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
	onHeightChange
}: {
	onBack: () => void;
	onHeightChange: (height: number) => void;
}) {
	const [state, setState] = useState<TrayState | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [editing, setEditing] = useState(false);
	const [menuOpen, setMenuOpen] = useState(false);
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
