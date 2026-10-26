import { BatteryWarning, Bluetooth } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag, useStoredString } from "./useStoredSetting";

/**
 * Settings > Notch > Bluetooth: the battery bubble beside the notch, shown
 * while a Bluetooth device is connected, and the low-battery warning.
 */
const LEVELS: [string, string][] = [
	["0", "Off"],
	["10", "10 %"],
	["15", "15 %"],
	["20", "20 %"],
	["25", "25 %"],
	["30", "30 %"],
	["40", "40 %"],
	["50", "50 %"]
];

export function BluetoothBatterySettings() {
	const [bubble, setBubble] = useStoredFlag("bloom-bt-bubble", true);
	const [low, setLow] = useStoredString("bloom-bt-low-battery", "20");
	return (
		<>
			<div className="setting-group-label">Bluetooth Battery</div>
			<div className="setting-group">
				<SettingRow
					icon={Bluetooth}
					label="Battery Bubble"
					desc="Beside the notch while a Bluetooth device is connected: its type and battery. Click it for the Bluetooth page"
				>
					<label className="toggle-switch">
						<input type="checkbox" checked={bubble} onChange={() => setBubble(!bubble)} />
						<span className="slider"></span>
					</label>
				</SettingRow>
				<SettingRow
					icon={BatteryWarning}
					label="Low Battery Warning"
					desc={
						low === "0"
							? "No warning"
							: `A notification when a connected device is at ${low} % or less (once, until it is charged)`
					}
					divider={false}
				>
					<select
						className="settings-select"
						value={LEVELS.some(([v]) => v === low) ? low : "20"}
						onChange={(e) => setLow(e.target.value)}
					>
						{LEVELS.map(([value, label]) => (
							<option key={value} value={value}>
								{label}
							</option>
						))}
					</select>
				</SettingRow>
			</div>
			<div className="sb-note">
				Battery levels come from Windows (as in Settings), else from the device itself: Bluetooth LE
				devices that report it, and Xbox controllers, which also say when they are charging. A
				device that reports nothing shows an empty battery.
			</div>
		</>
	);
}
