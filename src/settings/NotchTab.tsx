import {
	PanelTop,
	Calendar,
	BellRing,
	Music,
	Minimize2,
	LayoutList,
	Sparkles,
	Circle,
	CloudSun,
	Timer,
	Archive,
	MousePointer2,
	Send,
	X
} from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag } from "./useStoredSetting";
import { ControlCenterSettings } from "./ControlCenterSettings";
import { TraySettings } from "./TraySettings";
import { NotchPagesSettings } from "./NotchPagesSettings";
import { StatusWidgetConfig } from "../components/StatusWidgetConfig";
import type { WidgetConfig } from "./types";

/** Settings > Notch's sub-sections. */
export type NotchSection =
	| "behavior"
	| "pages"
	| "calendar"
	| "music"
	| "shelf"
	| "weather"
	| "widgets"
	| "controls"
	| "tray";

interface NotchTabProps {
	section: NotchSection;
	notchMode: string;
	setNotchModeValue: (mode: string) => void;
	notchEdgeDelay: number;
	handleNotchEdgeDelayChange: (val: number) => void;
	calendarEnabled: boolean;
	timerSoundEnabled: boolean;
	toggleTimerSound: () => void;
	musicModeEnabled: boolean;
	toggleMusicMode: () => void;
	musicCompactNotch: boolean;
	toggleMusicCompactNotch: () => void;
	notchWidgetsOnHover: boolean;
	toggleNotchWidgetsOnHover: () => void;
	shelfPersist: boolean;
	toggleShelfPersist: () => void;
	mediaLayout: "classic" | "compact";
	toggleMediaLayout: (layout: "classic" | "compact") => void;
	mediaAmbienceEnabled: boolean;
	toggleAmbience: () => void;
	mediaCompactGlowEnabled: boolean;
	toggleCompactGlow: () => void;
	weatherEnabled: boolean;
	toggleWeather: () => void;
	tempUnitFahrenheit: boolean;
	toggleTempUnit: () => void;
	cityName: string;
	setCityName: (name: string) => void;
	citySearchResults: Array<{ name: string; country: string; latitude: number; longitude: number }>;
	showCityDropdown: boolean;
	setShowCityDropdown: (show: boolean) => void;
	selectCity: (city: {
		name: string;
		country: string;
		latitude: number;
		longitude: number;
	}) => void;
	handleCityClear: () => void;
	statusWidgets: WidgetConfig;
	handleWidgetsChange: (config: WidgetConfig) => void;
}

export function NotchTab({
	section,
	notchMode,
	setNotchModeValue,
	notchEdgeDelay,
	handleNotchEdgeDelayChange,
	calendarEnabled,
	timerSoundEnabled,
	toggleTimerSound,
	musicModeEnabled,
	toggleMusicMode,
	musicCompactNotch,
	toggleMusicCompactNotch,
	notchWidgetsOnHover,
	toggleNotchWidgetsOnHover,
	shelfPersist,
	toggleShelfPersist,
	mediaLayout,
	toggleMediaLayout,
	mediaAmbienceEnabled,
	toggleAmbience,
	mediaCompactGlowEnabled,
	toggleCompactGlow,
	weatherEnabled,
	toggleWeather,
	tempUnitFahrenheit,
	toggleTempUnit,
	cityName,
	setCityName,
	citySearchResults,
	showCityDropdown,
	setShowCityDropdown,
	selectCity,
	handleCityClear,
	statusWidgets,
	handleWidgetsChange
}: NotchTabProps) {
	const [shelfLocalSend, setShelfLocalSend] = useStoredFlag("bloom-shelf-localsend", true);

	if (section === "pages") return <NotchPagesSettings />;
	if (section === "controls") return <ControlCenterSettings />;
	if (section === "tray") return <TraySettings />;

	if (section === "calendar") {
		return (
			<>
				<div className="setting-group-label">Timer</div>
				<div className="setting-group">
					<SettingRow
						icon={BellRing}
						label="Timer Sound"
						desc="Play a chime when the timer finishes"
						divider={false}
					>
						<label className="toggle-switch">
							<input type="checkbox" checked={timerSoundEnabled} onChange={toggleTimerSound} />
							<span className="slider"></span>
						</label>
					</SettingRow>
				</div>
				{!calendarEnabled && (
					<div className="setting-group-note">
						<Calendar size={11} /> The Calendar & Timer page is off; switch it on in Pages.
					</div>
				)}
			</>
		);
	}

	if (section === "music") {
		return (
			<>
				<div className="setting-group-label">Music</div>
				<div className="setting-group">
					<SettingRow
						icon={Music}
						label="Music Mode"
						desc="Interactive live music widget"
						divider={musicModeEnabled}
					>
						<label className="toggle-switch">
							<input type="checkbox" checked={musicModeEnabled} onChange={toggleMusicMode} />
							<span className="slider"></span>
						</label>
					</SettingRow>

					{musicModeEnabled && (
						<>
							<SettingRow
								icon={Minimize2}
								label="Compact Mode"
								desc="Show visualizer & artwork when collapsed"
							>
								<label className="toggle-switch">
									<input
										type="checkbox"
										checked={musicCompactNotch}
										onChange={toggleMusicCompactNotch}
									/>
									<span className="slider"></span>
								</label>
							</SettingRow>

							<SettingRow icon={LayoutList} label="Media Layout" desc="Choose expanded player style">
								<div className="unit-toggle-minimal wide">
									<span
										className={mediaLayout === "classic" ? "active" : ""}
										onClick={() => toggleMediaLayout("classic")}
									>
										Classic
									</span>
									<span
										className={mediaLayout === "compact" ? "active" : ""}
										onClick={() => toggleMediaLayout("compact")}
									>
										Compact
									</span>
								</div>
							</SettingRow>

							<SettingRow
								icon={Sparkles}
								label="Ambient Glow"
								desc="Colored glow behind expanded album art"
							>
								<label className="toggle-switch">
									<input type="checkbox" checked={mediaAmbienceEnabled} onChange={toggleAmbience} />
									<span className="slider"></span>
								</label>
							</SettingRow>

							<SettingRow
								icon={Circle}
								label="Compact Glow"
								desc="Glow around collapsed thumbnail"
								divider={false}
							>
								<label className="toggle-switch">
									<input
										type="checkbox"
										checked={mediaCompactGlowEnabled}
										onChange={toggleCompactGlow}
									/>
									<span className="slider"></span>
								</label>
							</SettingRow>
						</>
					)}
				</div>
			</>
		);
	}

	if (section === "shelf") {
		return (
			<>
				<div className="setting-group-label">Shelf</div>
				<div className="setting-group">
					<SettingRow
						icon={Archive}
						label="Keep Items"
						desc="Keep shelf items after Bloom closes"
					>
						<label className="toggle-switch">
							<input type="checkbox" checked={shelfPersist} onChange={toggleShelfPersist} />
							<span className="slider"></span>
						</label>
					</SettingRow>
					<SettingRow
						icon={Send}
						label="LocalSend"
						desc={
							shelfLocalSend
								? "A LocalSend button under the shelf's items sends the ones you pick to a nearby device"
								: "No LocalSend button in the shelf"
						}
						divider={false}
					>
						<label className="toggle-switch">
							<input
								type="checkbox"
								checked={shelfLocalSend}
								onChange={() => setShelfLocalSend(!shelfLocalSend)}
							/>
							<span className="slider"></span>
						</label>
					</SettingRow>
				</div>
				{shelfLocalSend && (
					<div className="setting-group-note">
						Open LocalSend on the other device to see it. Devices are only looked for while
						you pick files, and nothing needs LocalSend open on this PC.
					</div>
				)}
			</>
		);
	}

	if (section === "weather") {
		return (
			<>
				<div className="setting-group-label">Weather</div>
				<div className="setting-group">
					<SettingRow
						icon={CloudSun}
						label="Weather Status"
						desc={cityName || "Auto-detect location"}
						divider={false}
					>
						<div className="weather-controls">
							<div className="unit-toggle-minimal" onClick={toggleTempUnit}>
								<span className={!tempUnitFahrenheit ? "active" : ""}>C</span>
								<span className={tempUnitFahrenheit ? "active" : ""}>F</span>
							</div>
							<label className="toggle-switch">
								<input type="checkbox" checked={weatherEnabled} onChange={toggleWeather} />
								<span className="slider"></span>
							</label>
						</div>
					</SettingRow>

					{weatherEnabled && (
						<div className="manual-city-input">
							<div className="city-input-row">
								<input
									type="text"
									placeholder="Search city..."
									value={cityName}
									onChange={(e) => setCityName(e.target.value)}
									onFocus={() => citySearchResults.length > 0 && setShowCityDropdown(true)}
									onKeyDown={(e) => {
										if (e.key === "Enter" && citySearchResults.length > 0) {
											e.preventDefault();
											selectCity(citySearchResults[0]);
										}
										if (e.key === "Escape") {
											setShowCityDropdown(false);
										}
									}}
									onBlur={() => setTimeout(() => setShowCityDropdown(false), 150)}
								/>
								{cityName && (
									<button
										className="city-clear-btn"
										onMouseDown={(e) => {
											e.preventDefault();
											handleCityClear();
										}}
										title="Clear city"
									>
										<X size={10} strokeWidth={2.5} />
									</button>
								)}
							</div>
							{showCityDropdown && citySearchResults.length > 0 && (
								<div className="city-dropdown">
									{citySearchResults.map((city) => (
										<button
											key={`${city.name}-${city.latitude}`}
											className="city-dropdown-item"
											onMouseDown={(e) => {
												e.preventDefault();
												selectCity(city);
											}}
										>
											<span className="city-dropdown-name">{city.name}</span>
											<span className="city-dropdown-country">{city.country}</span>
										</button>
									))}
								</div>
							)}
						</div>
					)}
				</div>
			</>
		);
	}

	if (section === "widgets") {
		return (
			<>
				<div className="setting-group-label">Status Widgets</div>
				<div className="setting-group">
					<SettingRow
						icon={MousePointer2}
						label="Only Show Widgets on Hover"
						desc={
							notchWidgetsOnHover
								? "Widgets appear when you hover the notch"
								: "Widgets show whenever the notch is visible"
						}
					>
						<label className="toggle-switch">
							<input
								type="checkbox"
								checked={notchWidgetsOnHover}
								onChange={toggleNotchWidgetsOnHover}
							/>
							<span className="slider"></span>
						</label>
					</SettingRow>
					<StatusWidgetConfig value={statusWidgets} onChange={handleWidgetsChange} />
				</div>
			</>
		);
	}

	return (
		<>
			<div className="setting-group-label">Notch</div>
			<div className="setting-group">
				<SettingRow
					icon={PanelTop}
					label="Notch Behavior"
					desc="Choose how the notch appears"
					divider={notchMode !== "fixed"}
				>
					<select
						className="settings-select"
						value={notchMode}
						onChange={(e) => setNotchModeValue(e.target.value)}
					>
						<option value="fixed">Fixed</option>
						<option value="smart">Smart</option>
						<option value="peek">Peek</option>
					</select>
				</SettingRow>

				{notchMode !== "fixed" && (
					<SettingRow
						icon={Timer}
						label="Peek Delay"
						desc={
							notchEdgeDelay === 0
								? "Reveal instantly on top-edge contact"
								: `Rest on the top edge for ${notchEdgeDelay}ms to reveal`
						}
						divider={false}
					>
						<input
							type="range"
							min="0"
							max="800"
							step="50"
							value={notchEdgeDelay}
							onChange={(e) => handleNotchEdgeDelayChange(parseInt(e.target.value))}
							className="settings-slider"
						/>
					</SettingRow>
				)}
			</div>
		</>
	);
}
