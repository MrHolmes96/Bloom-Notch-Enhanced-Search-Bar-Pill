import { MonitorPlay } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredString } from "./useStoredSetting";

/** Settings > Dock: static snapshots or Windows' live previews. */
export function PreviewStyleRow() {
	const [style, setStyle] = useStoredString("bloom-dock-preview-style", "static");
	return (
		<SettingRow
			icon={MonitorPlay}
			label="Preview Style"
			desc={
				style === "live"
					? "Live, moving previews like the Windows taskbar"
					: "A snapshot of each window"
			}
		>
			<select className="settings-select" value={style} onChange={(e) => setStyle(e.target.value)}>
				<option value="static">Static</option>
				<option value="live">Live</option>
			</select>
		</SettingRow>
	);
}
