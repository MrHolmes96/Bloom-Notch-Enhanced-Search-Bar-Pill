import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Download, EyeOff, LayoutGrid, ListX, RotateCcw, Zap } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag, useStoredString } from "./useStoredSetting";
import {
	TRAY_FOLLOW_KEY,
	TRAY_PLACEMENT_KEY,
	iconLabels,
	parsePlacements,
	visibilityOf,
	withVisibility,
	type TrayIconInfo,
	type TrayState
} from "../trayLayout";

/**
 * Settings > Notch > System Tray: which icons the notch's tray shows, and
 * Tray Icon Manager's hidden icons. Bloom only reads Tray Icon Manager's
 * settings; what is chosen here changes Bloom's tray, not Windows'.
 */

function AppImage({ path, image }: { path: string; image?: string | null }) {
	const [src, setSrc] = useState<string | null>(image ?? null);
	useEffect(() => {
		if (image) {
			setSrc(image);
			return;
		}
		let alive = true;
		void invoke<string | null>("get_app_icon", { path })
			.then((icon) => alive && setSrc(icon ?? null))
			.catch(() => {});
		return () => {
			alive = false;
		};
	}, [path, image]);
	return src ? (
		<img src={src} alt="" width={16} height={16} style={{ objectFit: "contain" }} />
	) : (
		<LayoutGrid size={14} strokeWidth={1.5} />
	);
}

export function TraySettings() {
	const [state, setState] = useState<TrayState | null>(null);
	const [placementsRaw, setPlacementsRaw] = useStoredString(TRAY_PLACEMENT_KEY, "{}");
	const [followManager, setFollowManager] = useStoredFlag(TRAY_FOLLOW_KEY, true);
	const [liveTray, setLiveTray] = useStoredFlag("bloom-tray-live", true);
	const [imported, setImported] = useState<number | null>(null);
	const placements = useMemo(() => parsePlacements(placementsRaw), [placementsRaw]);

	useEffect(() => {
		void invoke<TrayState>("get_tray_state")
			.then(setState)
			.catch(() => setState(null));
	}, []);

	const icons = useMemo(() => state?.icons ?? [], [state]);
	const labels = useMemo(() => iconLabels(icons), [icons]);
	const entries = state?.manager_entries ?? [];
	const running = entries.filter((e) => e.running).length;
	const notRunning = entries.filter((e) => !e.running);
	const changed = Object.keys(placements).length;

	const setShown = (icon: TrayIconInfo, shown: boolean) =>
		setPlacementsRaw(
			JSON.stringify(withVisibility(placements, icon, shown ? "shown" : "hidden", followManager))
		);

	// Bloom keeps its own copy, so these stay hidden even without Tray Icon
	// Manager (or with following it turned off).
	const importManager = () => {
		if (!state) return;
		const next = { ...placements };
		let count = 0;
		for (const icon of state.icons) {
			if (icon.manager_hidden && next[icon.key] !== "hidden") {
				next[icon.key] = "hidden";
				count++;
			}
		}
		for (const entry of state.manager_entries) {
			if (
				entry.key &&
				!state.icons.some((i) => i.key === entry.key) &&
				next[entry.key] !== "hidden"
			) {
				next[entry.key] = "hidden";
				count++;
			}
		}
		setPlacementsRaw(JSON.stringify(next));
		setImported(count);
	};

	return (
		<>
			<div className="setting-group-label">System Tray</div>
			<div className="setting-group">
				<SettingRow
					icon={Zap}
					label="Live Tray"
					desc={
						liveTray
							? "Icons show and update instantly, with their own pictures, and clicks open at once"
							: "Icons are read from Windows' tray when it opens (takes effect after restarting Bloom)"
					}
				>
					<label className="toggle-switch">
						<input
							type="checkbox"
							checked={liveTray}
							onChange={(e) => setLiveTray(e.target.checked)}
						/>
						<span className="slider"></span>
					</label>
				</SettingRow>
				<SettingRow
					icon={EyeOff}
					label="Follow Tray Icon Manager"
					desc={
						!state
							? "Looking for Tray Icon Manager…"
							: !state.manager_found
								? "Tray Icon Manager's settings weren't found on this PC"
								: entries.length === 0
									? "Tray Icon Manager hides no icons right now"
									: `It hides ${entries.length} icon${entries.length === 1 ? "" : "s"}, ${running} of them in the tray now: hidden here too`
					}
				>
					<label className="toggle-switch">
						<input
							type="checkbox"
							checked={followManager}
							onChange={(e) => setFollowManager(e.target.checked)}
						/>
						<span className="slider"></span>
					</label>
				</SettingRow>
				{notRunning.length > 0 && (
					<SettingRow
						icon={ListX}
						label="Also Hidden, Not Running Now"
						desc={notRunning.map((e) => e.name).join(", ")}
					/>
				)}
				{state?.manager_found && entries.length > 0 && (
					<SettingRow
						icon={Download}
						label="Copy Tray Icon Manager's Hidden Icons"
						desc={
							imported === null
								? "Keeps them hidden in Bloom even without Tray Icon Manager"
								: imported === 0
									? "Already copied"
									: `Copied ${imported} icon${imported === 1 ? "" : "s"}`
						}
						action
						onClick={importManager}
					/>
				)}
				{icons.map((icon) => {
					const label = labels.get(icon.id) ?? icon.name;
					const shown = visibilityOf(icon, placements, followManager) === "shown";
					return (
						<div key={icon.id}>
							<div className="setting-item">
								<div className="setting-icon-bg">
									<AppImage path={icon.icon_path ?? icon.path} image={icon.icon_path ? null : icon.image} />
								</div>
								<div className="setting-info">
									<span className="setting-label">{label}</span>
									<span className="setting-desc">
										{shown
											? "Shown in the tray"
											: icon.manager_hidden && placements[icon.key] === undefined
												? "Hidden by Tray Icon Manager"
												: icon.hidden_in_windows && placements[icon.key] === undefined
													? "Hidden in Windows' tray"
													: "Hidden"}
									</span>
								</div>
								<label className="toggle-switch">
									<input
										type="checkbox"
										checked={shown}
										onChange={(e) => setShown(icon, e.target.checked)}
									/>
									<span className="slider"></span>
								</label>
							</div>
							<div className="setting-divider" />
						</div>
					);
				})}
				{state && icons.length === 0 && (
					<SettingRow
						icon={LayoutGrid}
						label="No Tray Icons Right Now"
						desc="Apps show here while they have an icon in the tray"
					/>
				)}
				<SettingRow
					icon={RotateCcw}
					label="Forget Changes Made in Bloom"
					desc={
						changed === 0
							? "Only Tray Icon Manager's hidden icons are hidden"
							: `${changed} icon${changed === 1 ? "" : "s"} shown or hidden here`
					}
					action={changed > 0}
					divider={false}
					onClick={changed > 0 ? () => setPlacementsRaw("{}") : undefined}
				/>
			</div>
		</>
	);
}
