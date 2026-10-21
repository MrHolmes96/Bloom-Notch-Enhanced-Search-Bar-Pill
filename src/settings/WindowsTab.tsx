import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AppWindow, Eraser } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag } from "./useStoredSetting";

/** Settings > Windows' sub-sections. */
export type WindowsSection = "sizes";

/** How Bloom treats other apps' windows. */
export function WindowsTab(_props: { section: WindowsSection }) {
	const [windowMemory, setWindowMemory] = useStoredFlag("bloom-window-memory", true);
	const [forgotten, setForgotten] = useState(false);

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
