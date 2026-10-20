import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AppWindow, Eraser, LayoutPanelLeft } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag } from "./useStoredSetting";

/** Settings > Windows' sub-sections. */
export type WindowsSection = "borders" | "sizes";

/** How Bloom treats other apps' windows. */
export function WindowsTab({ section }: { section: WindowsSection }) {
	const [snapBorders, setSnapBorders] = useStoredFlag("bloom-windows-snap-borders", true);
	const [windowMemory, setWindowMemory] = useStoredFlag("bloom-window-memory", true);
	const [forgotten, setForgotten] = useState(false);

	if (section === "sizes") {
		return (
			<>
				<div className="setting-group-label">Window Sizes</div>
				<div className="setting-group">
					<SettingRow
						icon={AppWindow}
						label="Remember Window Sizes"
						desc="Apps open at the size and place you last left them"
						divider={windowMemory}
					>
						<label className="toggle-switch">
							<input
								type="checkbox"
								checked={windowMemory}
								onChange={() => setWindowMemory(!windowMemory)}
							/>
							<span className="slider"></span>
						</label>
					</SettingRow>
					{windowMemory && (
						<SettingRow
							icon={Eraser}
							label="Forget Saved Sizes"
							desc={forgotten ? "Forgotten" : "Let every app choose its own size again"}
							action
							divider={false}
							onClick={() => {
								void invoke("forget_window_sizes");
								setForgotten(true);
								setTimeout(() => setForgotten(false), 2500);
							}}
						/>
					)}
				</div>
			</>
		);
	}

	return (
		<>
			<div className="setting-group-label">Borders</div>
			<div className="setting-group">
				<SettingRow
					icon={LayoutPanelLeft}
					label="No Border on Snapped Windows"
					desc={
						snapBorders
							? "Windows snapped to a half, third or quarter, or by Snap Layouts, lose Windows 11's border, as in Windows 10"
							: "Snapped windows keep Windows 11's border"
					}
					divider={false}
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
			</div>
			<div className="setting-group-note">
				A window that's no longer snapped looks as Windows' own setting says: no border, or the
				accent border when "Show accent colour on title bars and window borders" is on.
			</div>
		</>
	);
}
