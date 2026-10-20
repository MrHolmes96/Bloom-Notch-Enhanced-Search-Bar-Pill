import { Palette, Droplet, Contrast, Droplets, Sun, Square, Maximize2, PanelBottom } from "lucide-react";
import { SettingRow } from "./SettingRow";

/** Settings > Appearance's sub-sections. */
export type AppearanceSection = "theme" | "colors" | "size";

interface AppearanceTabProps {
	section: AppearanceSection;
	themeMode: string;
	handleThemeModeChange: (mode: string) => void;
	themeColor: string;
	handleThemeColorChange: (color: string) => void;
	themeOpacity: number;
	handleOpacityChange: (val: number) => void;
	themeSaturation: number;
	handleSaturationChange: (val: number) => void;
	themeBrightness: number;
	handleBrightnessChange: (val: number) => void;
	cornersEnabled: boolean;
	toggleCorners: () => void;
	notchScale: number;
	handleNotchScaleChange: (val: number) => void;
	dockScale: number;
	handleDockScaleChange: (val: number) => void;
}

const MIN_SCALE = 0.8;
const MAX_SCALE = 1.3;

/** The − 100 % + control for one size. */
function ScaleControl({
	value,
	onChange,
	what
}: {
	value: number;
	onChange: (val: number) => void;
	what: string;
}) {
	return (
		<div className="scale-button-container">
			<button
				onClick={() => onChange(Math.max(MIN_SCALE, parseFloat((value - 0.1).toFixed(1))))}
				disabled={value <= MIN_SCALE}
				className="scale-adjust-btn"
				title={`Make the ${what} smaller`}
			>
				—
			</button>
			<span className="scale-display-value">{Math.round(value * 100)}%</span>
			<button
				onClick={() => onChange(Math.min(MAX_SCALE, parseFloat((value + 0.1).toFixed(1))))}
				disabled={value >= MAX_SCALE}
				className="scale-adjust-btn"
				title={`Make the ${what} larger`}
			>
				+
			</button>
		</div>
	);
}

export function AppearanceTab({
	section,
	themeMode,
	handleThemeModeChange,
	themeColor,
	handleThemeColorChange,
	themeOpacity,
	handleOpacityChange,
	themeSaturation,
	handleSaturationChange,
	themeBrightness,
	handleBrightnessChange,
	cornersEnabled,
	toggleCorners,
	notchScale,
	handleNotchScaleChange,
	dockScale,
	handleDockScaleChange
}: AppearanceTabProps) {
	const showCustomColor = themeMode === "custom";
	const showAdvancedSliders = themeMode === "custom" || themeMode === "adaptive";

	return (
		<>
			{section === "theme" && (
				<>
					<div className="setting-group-label">Theme</div>
					<div className="setting-group">
						<SettingRow
							icon={Palette}
							label="Theme Mode"
							desc="Configure visual styling"
							divider={showCustomColor}
						>
							<select
								className="settings-select"
								value={themeMode}
								onChange={(e) => handleThemeModeChange(e.target.value)}
							>
								<option value="dark">Dark (Translucent)</option>
								<option value="light">Light (Translucent)</option>
								<option value="custom">Custom Color</option>
								<option value="adaptive">Adaptive Accent</option>
							</select>
						</SettingRow>

						{showCustomColor && (
							<SettingRow
								icon={Droplet}
								label="Custom Theme Color"
								desc="Choose layout background color"
								divider={false}
							>
								<div className="color-picker-row">
									<input
										type="color"
										value={themeColor}
										onChange={(e) => handleThemeColorChange(e.target.value)}
										className="color-picker-input"
									/>
									<span className="color-picker-label">{themeColor.toUpperCase()}</span>
								</div>
							</SettingRow>
						)}
					</div>
				</>
			)}

			{section === "colors" && (
				<>
					<div className="setting-group-label">Background</div>
					<div className="setting-group">
						<SettingRow
							icon={Contrast}
							label="Background Opacity"
							desc={`Adjust theme transparency (${Math.round(themeOpacity * 100)}%)`}
							divider={showAdvancedSliders}
						>
							<input
								type="range"
								min="0.1"
								max="1.0"
								step="0.05"
								value={themeOpacity}
								onChange={(e) => handleOpacityChange(parseFloat(e.target.value))}
								className="settings-slider"
							/>
						</SettingRow>

						{showAdvancedSliders && (
							<>
								<SettingRow
									icon={Droplets}
									label="Color Saturation"
									desc={`Adjust theme color vibrancy (${Math.round(themeSaturation * 100)}%)`}
								>
									<input
										type="range"
										min="0.0"
										max="1.0"
										step="0.02"
										value={themeSaturation}
										onChange={(e) => handleSaturationChange(parseFloat(e.target.value))}
										className="settings-slider"
									/>
								</SettingRow>

								<SettingRow
									icon={Sun}
									label="Background Brightness"
									desc={`Adjust background lightness (${Math.round(themeBrightness * 100)}%)`}
									divider={false}
								>
									<input
										type="range"
										min="0.0"
										max="1.0"
										step="0.02"
										value={themeBrightness}
										onChange={(e) => handleBrightnessChange(parseFloat(e.target.value))}
										className="settings-slider"
									/>
								</SettingRow>
							</>
						)}
					</div>
					{!showAdvancedSliders && (
						<div className="setting-group-note">
							Color saturation and brightness can be set with the Custom Color or Adaptive Accent theme.
						</div>
					)}
				</>
			)}

			{section === "size" && (
				<>
					<div className="setting-group-label">Display</div>
					<div className="setting-group">
						<SettingRow icon={Square} label="Screen Corners" desc="Rounded top edges" divider={false}>
							<label className="toggle-switch">
								<input type="checkbox" checked={cornersEnabled} onChange={toggleCorners} />
								<span className="slider"></span>
							</label>
						</SettingRow>
					</div>

					<div className="setting-group-label">UI Size</div>
					<div className="setting-group">
						<SettingRow
							icon={Maximize2}
							label="Notch Size"
							desc="The notch with its text, the volume and brightness pop-ups, and this window"
						>
							<ScaleControl value={notchScale} onChange={handleNotchScaleChange} what="notch" />
						</SettingRow>
						<SettingRow
							icon={PanelBottom}
							label="Dock Size"
							desc="The dock, its icons and its menus"
							divider={false}
						>
							<ScaleControl value={dockScale} onChange={handleDockScaleChange} what="dock" />
						</SettingRow>
					</div>
				</>
			)}
		</>
	);
}
