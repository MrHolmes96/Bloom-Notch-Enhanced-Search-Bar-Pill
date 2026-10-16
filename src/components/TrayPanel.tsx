import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronLeft, ExternalLink, RefreshCw } from "lucide-react";
import "./QuickPanel.css";
import "./TrayPanel.css";

/**
 * The system tray inside the notch, opened from the control center's tray
 * button: one tile per tray icon. Click runs the icon's default action,
 * right-click opens the app's own tray menu, as on the taskbar.
 */

interface TrayApp {
	name: string;
	path: string;
	tray_ids: string[];
	window_handles: number[];
}

interface Tile {
	id: string;
	name: string;
	path: string;
}

export const TRAY_PER_ROW = 5;
export const TRAY_TILE_HEIGHT = 64;

/** Panel height inside the open notch, below its 36 px header. */
export function trayPanelHeight(count: number) {
	const rows = Math.max(1, Math.min(3, Math.ceil(count / TRAY_PER_ROW)));
	return 4 + 26 + 6 + rows * TRAY_TILE_HEIGHT + (rows - 1) * 6 + 12;
}

const iconCache = new Map<string, string | null>();

function TrayTile({ tile, onError }: { tile: Tile; onError: (text: string) => void }) {
	const [icon, setIcon] = useState<string | null>(() => iconCache.get(tile.path) ?? null);

	useEffect(() => {
		if (iconCache.has(tile.path)) return;
		void invoke<string | null>("get_app_icon", { path: tile.path })
			.then((value) => {
				iconCache.set(tile.path, value ?? null);
				setIcon(value ?? null);
			})
			.catch(() => iconCache.set(tile.path, null));
	}, [tile.path]);

	return (
		<button
			type="button"
			className="tray-tile"
			title={`${tile.name}: click to open, right-click for its menu`}
			onClick={(e) => {
				e.stopPropagation();
				invoke("activate_tray_icon", { trayId: tile.id }).catch((err) => onError(String(err)));
			}}
			onContextMenu={(e) => {
				e.preventDefault();
				e.stopPropagation();
				invoke("show_tray_context_menu", { trayId: tile.id }).catch((err) => onError(String(err)));
			}}
		>
			{icon ? (
				<img className="tray-tile-icon" src={icon} alt="" draggable={false} />
			) : (
				<span className="tray-tile-icon tray-tile-letter">{tile.name[0]}</span>
			)}
			<span className="tray-tile-name">{tile.name}</span>
		</button>
	);
}

export function TrayPanel({
	onBack,
	onCountChange
}: {
	onBack: () => void;
	onCountChange: (count: number) => void;
}) {
	const [tiles, setTiles] = useState<Tile[] | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const load = async () => {
		setLoading(true);
		try {
			const apps = await invoke<TrayApp[]>("get_tray_apps");
			const next: Tile[] = [];
			for (const app of apps) {
				app.tray_ids.forEach((id, index) =>
					next.push({
						id,
						path: app.path,
						name: app.tray_ids.length > 1 ? `${app.name} ${index + 1}` : app.name
					})
				);
			}
			next.sort((a, b) => a.name.localeCompare(b.name));
			setTiles(next);
			onCountChange(next.length);
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

	return (
		<div className="qp" onClick={(e) => e.stopPropagation()}>
			<div className="qp-header">
				<button className="qp-icon-btn" onClick={onBack} aria-label="Back to controls">
					<ChevronLeft />
				</button>
				<span className="qp-title">System Tray</span>
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
			{tiles && tiles.length === 0 && !error && (
				<div className="tray-message">No apps are in the tray right now</div>
			)}
			{tiles && tiles.length > 0 && (
				<div className="tray-grid">
					{tiles.map((tile) => (
						<TrayTile key={tile.id} tile={tile} onError={setError} />
					))}
				</div>
			)}
		</div>
	);
}
