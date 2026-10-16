import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Download, EyeOff, LayoutGrid, RotateCcw } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag, useStoredString } from "./useStoredSetting";
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

/**
 * Settings > Notch > System Tray: where each icon shows in the notch's tray,
 * and Tray Icon Manager's hidden icons. Bloom only reads Tray Icon Manager's
 * settings; what is chosen here changes Bloom's tray, not Windows'.
 */

function AppImage({ path }: { path: string }) {
	const [src, setSrc] = useState<string | null>(null);
	useEffect(() => {
		let alive = true;
		void invoke<string | null>("get_app_icon", { path })
			.then((icon) => alive && setSrc(icon ?? null))
			.catch(() => {});
		return () => {
			alive = false;
		};
	}, [path]);
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
	const [imported, setImported] = useState<number | null>(null);
	const placements = useMemo(() => parsePlacements(placementsRaw), [placementsRaw]);

	useEffect(() => {
		void invoke<TrayState>("get_tray_state")
			.then(setState)
			.catch(() => setState(null));
	}, []);

	const icons = state?.icons ?? [];
	const labels = useMemo(() => iconLabels(icons), [icons]);
	const managerCount = state?.manager_entries.length ?? 0;
	const changed = Object.keys(placements).length;

	const setPlace = (icon: TrayIconInfo, place: Placement) =>
		setPlacementsRaw(JSON.stringify(withPlacement(placements, icon, place, followManager)));

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
					icon={EyeOff}
					label="Follow Tray Icon Manager"
					desc={
						!state
							? "Looking for Tray Icon Manager…"
							: state.manager_found
								? `Icons Tray Icon Manager hides stay hidden in Bloom's tray (${managerCount})`
								: "Tray Icon Manager isn't set up on this PC"
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
				{state?.manager_found && managerCount > 0 && (
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
					return (
						<div key={icon.id}>
							<div className="setting-item">
								<div className="setting-icon-bg">
									<AppImage path={icon.path} />
								</div>
								<div className="setting-info">
									<span className="setting-label">{label}</span>
									<span className="setting-desc">
										{icon.manager_hidden
											? "Hidden by Tray Icon Manager"
											: icon.promoted
												? "On Windows' taskbar"
												: "In Windows' hidden icons"}
									</span>
								</div>
								<select
									className="settings-select"
									value={placementOf(icon, placements, followManager)}
									onChange={(e) => setPlace(icon, e.target.value as Placement)}
								>
									<option value="tray">In the tray</option>
									<option value="menu">Hidden icons</option>
									<option value="hidden">Not shown</option>
								</select>
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
					label="Use Windows' Placement"
					desc={
						changed === 0
							? "Every icon is where Windows has it"
							: `Forget what was changed in Bloom (${changed} icon${changed === 1 ? "" : "s"})`
					}
					action={changed > 0}
					divider={false}
					onClick={changed > 0 ? () => setPlacementsRaw("{}") : undefined}
				/>
			</div>
		</>
	);
}
