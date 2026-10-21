import { invoke } from "@tauri-apps/api/core";
import { Contrast, Droplet, LayoutPanelLeft } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag, useStoredString } from "./useStoredSetting";

const DEFAULT_COLOR = "#808080";

/** An opacity setting in percent, 0 to 100. */
function readOpacity(value: string): number {
	const parsed = Number.parseFloat(value);
	return Number.isFinite(parsed) ? Math.min(100, Math.max(0, Math.round(parsed))) : 0;
}

/**
 * Settings > Appearance > Window Borders: snapped windows lose Windows 11's
 * border and can get Bloom's own instead, one screen pixel wide, in any
 * colour and opacity (0 % is no border at all).
 */
export function WindowBordersSettings() {
	const [snapBorders, setSnapBorders] = useStoredFlag("bloom-windows-snap-borders", true);
	const [color, setColor] = useStoredString("bloom-windows-snap-border-color", DEFAULT_COLOR);
	const [opacityText, setOpacityText] = useStoredString("bloom-windows-snap-border-opacity", "0");
	const opacity = readOpacity(opacityText);
	const hex = /^#[0-9a-f]{6}$/i.test(color) ? color : DEFAULT_COLOR;

	const useAccent = () => {
		invoke<string>("get_system_accent_color")
			.then((accent) => {
				if (/^#[0-9a-f]{6}$/i.test(accent)) setColor(accent.toLowerCase());
			})
			.catch(() => {});
	};

	return (
		<>
			<div className="setting-group-label">Snapped Windows</div>
			<div className="setting-group">
				<SettingRow
					icon={LayoutPanelLeft}
					label="Own Border on Snapped Windows"
					desc={
						snapBorders
							? "Windows snapped to a half, third or quarter, or by Snap Layouts, get the border below instead of Windows 11's"
							: "Snapped windows keep Windows 11's border"
					}
					divider={snapBorders}
				>
					<label className="toggle-switch">
						<input
							type="checkbox"
							checked={snapBorders}
							onChange={() => setSnapBorders(!snapBorders)}
						/>
						<span className="slider"></span>
					</label>
				</SettingRow>

				{snapBorders && (
					<>
						<SettingRow
							icon={Contrast}
							label="Border Opacity"
							desc={opacity === 0 ? "0 %: no border at all" : `${opacity} %, one pixel wide`}
						>
							<input
								type="range"
								min="0"
								max="100"
								step="5"
								value={opacity}
								onChange={(e) => setOpacityText(e.target.value)}
								className="settings-slider"
							/>
						</SettingRow>

						<SettingRow
							icon={Droplet}
							label="Border Colour"
							desc={opacity === 0 ? "Shows once the opacity is above 0 %" : "Any colour, or your Windows accent colour"}
							divider={false}
						>
							<div className="color-picker-row">
								<button className="border-accent-btn" onClick={useAccent} title="Use your Windows accent colour">
									Accent
								</button>
								<input
									type="color"
									value={hex}
									onChange={(e) => setColor(e.target.value)}
									className="color-picker-input"
								/>
								<span className="color-picker-label">{hex.toUpperCase()}</span>
							</div>
						</SettingRow>
					</>
				)}
			</div>
			<div className="setting-group-note">
				A window that's no longer snapped looks as Windows' own setting says: no border, or the
				accent border when "Show accent colour on title bars and window borders" is on.
			</div>
		</>
	);
}
