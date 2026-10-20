import { Power, Clock, BatteryWarning, RefreshCw, RotateCcw, LogOut } from "lucide-react";
import { SettingRow } from "./SettingRow";

/** Settings > General's sub-sections. */
export type GeneralSection = "startup" | "time" | "bloom";

interface GeneralTabProps {
	section: GeneralSection;
	autostart: boolean;
	toggleAutostart: () => void;
	timeFormat24h: boolean;
	toggleTimeFormat24h: () => void;
	lowBatteryThreshold: number;
	handleThresholdChange: (val: number) => void;
	restartBloom: () => void;
	resetToDefaults: () => void;
	quitBloom: () => void;
}

export function GeneralTab({
	section,
	autostart,
	toggleAutostart,
	timeFormat24h,
	toggleTimeFormat24h,
	lowBatteryThreshold,
	handleThresholdChange,
	restartBloom,
	resetToDefaults,
	quitBloom
}: GeneralTabProps) {
	if (section === "time") {
		return (
			<>
				<div className="setting-group-label">Time</div>
				<div className="setting-group">
					<SettingRow icon={Clock} label="24-Hour Time" desc="Use 24-hour clock format" divider={false}>
						<label className="toggle-switch">
							<input type="checkbox" checked={timeFormat24h} onChange={toggleTimeFormat24h} />
							<span className="slider"></span>
						</label>
					</SettingRow>
				</div>
				<div className="setting-group-label">Battery</div>
				<div className="setting-group">
					<SettingRow
						icon={BatteryWarning}
						label="Low Battery Alert"
						desc={`Trigger at ${lowBatteryThreshold}%`}
						divider={false}
					>
						<input
							type="range"
							min="5"
							max="50"
							step="5"
							value={lowBatteryThreshold}
							onChange={(e) => handleThresholdChange(parseInt(e.target.value))}
							className="settings-slider"
						/>
					</SettingRow>
				</div>
			</>
		);
	}
	if (section === "bloom") {
		return (
			<>
				<div className="setting-group-label">Bloom</div>
				<div className="setting-group">
					<SettingRow
						icon={RefreshCw}
						label="Restart Bloom"
						desc="Reinitialize all components"
						action
						onClick={restartBloom}
					/>
					<SettingRow
						icon={RotateCcw}
						label="Reset to Defaults"
						desc="Restore all settings and restart Bloom"
						action
						danger
						onClick={resetToDefaults}
					/>
					<SettingRow
						icon={LogOut}
						label="Quit Bloom"
						desc="Exit application completely"
						action
						danger
						onClick={quitBloom}
						divider={false}
					/>
				</div>
			</>
		);
	}
	return (
		<>
			<div className="setting-group-label">Startup</div>
			<div className="setting-group">
				<SettingRow
					icon={Power}
					label="Launch at Login"
					desc="Start with Windows, ahead of other startup apps"
					divider={false}
				>
					<label className="toggle-switch">
						<input type="checkbox" checked={autostart} onChange={toggleAutostart} />
						<span className="slider"></span>
					</label>
				</SettingRow>
			</div>
		</>
	);
}
