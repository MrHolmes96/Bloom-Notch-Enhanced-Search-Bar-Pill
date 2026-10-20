import { Volume2, Sun, SunMedium, ArrowLeftToLine, ArrowRightToLine } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredString } from "./useStoredSetting";

/** Settings > Overlays' sub-sections. */
export type OverlaysSection = "volume" | "brightness";

interface OverlaysTabProps {
	section: OverlaysSection;
	volumeOverlayEnabled: boolean;
	toggleVolumeOverlay: () => void;
	volumeEdgeEnabled: boolean;
	toggleVolumeEdge: () => void;
	brightnessOverlayEnabled: boolean;
	toggleBrightnessOverlay: () => void;
	brightnessEdgeEnabled: boolean;
	toggleBrightnessEdge: () => void;
}

export function OverlaysTab({
	section,
	volumeOverlayEnabled,
	toggleVolumeOverlay,
	volumeEdgeEnabled,
	toggleVolumeEdge,
	brightnessOverlayEnabled,
	toggleBrightnessOverlay,
	brightnessEdgeEnabled,
	toggleBrightnessEdge
}: OverlaysTabProps) {
	const [keyStep, setKeyStep] = useStoredString("bloom-brightness-key-step", "2");
	return (
		<>
			{section === "volume" && (
				<>
					<div className="setting-group-label">Volume</div>
					<div className="setting-group">
						<SettingRow
							icon={Volume2}
							label="Volume HUD"
							desc="Bloom volume overlay"
							divider={volumeOverlayEnabled}
						>
							<label className="toggle-switch">
								<input type="checkbox" checked={volumeOverlayEnabled} onChange={toggleVolumeOverlay} />
								<span className="slider"></span>
							</label>
						</SettingRow>

						{volumeOverlayEnabled && (
							<SettingRow
								icon={ArrowLeftToLine}
								label="Show on Edge Hover"
								desc="Slide in from left edge"
								divider={false}
							>
								<label className="toggle-switch">
									<input type="checkbox" checked={volumeEdgeEnabled} onChange={toggleVolumeEdge} />
									<span className="slider"></span>
								</label>
							</SettingRow>
						)}
					</div>
				</>
			)}

			{section === "brightness" && (
				<>
					<div className="setting-group-label">Brightness</div>
					<div className="setting-group">
						<SettingRow icon={Sun} label="Brightness HUD" desc="Bloom brightness overlay">
							<label className="toggle-switch">
								<input
									type="checkbox"
									checked={brightnessOverlayEnabled}
									onChange={toggleBrightnessOverlay}
								/>
								<span className="slider"></span>
							</label>
						</SettingRow>

						<SettingRow
							icon={SunMedium}
							label="Brightness Key Step"
							desc={
								keyStep === "0"
									? "Brightness keys use Windows' own step"
									: `Each press of a brightness key changes it by ${keyStep}%, on any keyboard`
							}
							divider={brightnessOverlayEnabled}
						>
							<select
								className="settings-select"
								value={keyStep}
								onChange={(e) => setKeyStep(e.target.value)}
							>
								<option value="1">1%</option>
								<option value="2">2%</option>
								<option value="5">5%</option>
								<option value="0">Windows default</option>
							</select>
						</SettingRow>

						{brightnessOverlayEnabled && (
							<SettingRow
								icon={ArrowRightToLine}
								label="Show on Edge Hover"
								desc="Slide in from right edge"
								divider={false}
							>
								<label className="toggle-switch">
									<input type="checkbox" checked={brightnessEdgeEnabled} onChange={toggleBrightnessEdge} />
									<span className="slider"></span>
								</label>
							</SettingRow>
						)}
					</div>
				</>
			)}
		</>
	);
}
