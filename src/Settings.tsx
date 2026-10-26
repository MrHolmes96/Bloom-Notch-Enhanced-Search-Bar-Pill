import { StrictMode, useState, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Effect } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
	X,
	Settings,
	Palette,
	PanelTop,
	Monitor,
	Layers,
	Bell,
	Search,
	Info,
	Blocks,
	Zap,
	Power,
	Clock,
	RefreshCw,
	Droplets,
	Maximize2,
	ListOrdered,
	Calendar,
	Music,
	Inbox,
	CloudSun,
	Gauge,
	LayoutGrid,
	PanelRightDashed,
	Eye,
	Sparkles,
	HardDrive,
	AppWindow,
	LayoutPanelLeft,
	Volume2,
	Sun,
	PanelRight,
	Globe,
	Puzzle,
	FolderSearch,
	SlidersHorizontal,
	Keyboard,
	Bluetooth
} from "lucide-react";
import {
	useSettings,
	GeneralTab,
	AppearanceTab,
	NotchTab,
	DockTab,
	OverlaysTab,
	SearchBarTab,
	NotificationsTab,
	IntegrationsTab,
	BlitzitSettings,
	AboutTab,
	WindowsTab
} from "./settings/index";
import type {
	SettingsTab,
	GeneralSection,
	AppearanceSection,
	NotchSection,
	DockSection,
	WindowsSection,
	OverlaysSection,
	NotificationsSection,
	SearchBarSection
} from "./settings/index";
import { initTheme } from "./theme";
import "./Settings.css";

const appWindow = getCurrentWebviewWindow();

type Icon = typeof Settings;

/**
 * The sidebar: Bloom's main categories, as before, and under the open one its
 * sub-sections, each a short page of the options that belong together.
 */
const CATEGORIES: {
	id: string;
	label: string;
	icon: Icon;
	sections: { id: string; label: string; icon: Icon }[];
}[] = [
	{
		id: "general",
		label: "General",
		icon: Settings,
		sections: [
			{ id: "startup", label: "Startup", icon: Power },
			{ id: "time", label: "Time & Battery", icon: Clock },
			{ id: "shortcuts", label: "Shortcuts", icon: Keyboard },
			{ id: "bloom", label: "Restart & Reset", icon: RefreshCw }
		]
	},
	{
		id: "appearance",
		label: "Appearance",
		icon: Palette,
		sections: [
			{ id: "theme", label: "Theme", icon: Palette },
			{ id: "colors", label: "Colors", icon: Droplets },
			{ id: "size", label: "Size & Corners", icon: Maximize2 },
			{ id: "borders", label: "Borders", icon: LayoutPanelLeft }
		]
	},
	{
		id: "notch",
		label: "Notch",
		icon: PanelTop,
		sections: [
			{ id: "behavior", label: "Behavior", icon: PanelTop },
			{ id: "pages", label: "Pages", icon: ListOrdered },
			{ id: "calendar", label: "Calendar", icon: Calendar },
			{ id: "music", label: "Music", icon: Music },
			{ id: "shelf", label: "Shelf", icon: Inbox },
			{ id: "weather", label: "Weather", icon: CloudSun },
			{ id: "widgets", label: "Status Widgets", icon: Gauge },
			{ id: "controls", label: "Controls", icon: LayoutGrid },
			{ id: "tray", label: "System Tray", icon: PanelRightDashed },
			{ id: "bluetooth", label: "Bluetooth", icon: Bluetooth }
		]
	},
	{
		id: "dock",
		label: "Dock",
		icon: Monitor,
		sections: [
			{ id: "behavior", label: "Behavior", icon: Eye },
			{ id: "look", label: "Look", icon: Sparkles },
			{ id: "extras", label: "Extras", icon: HardDrive }
		]
	},
	{
		id: "windows",
		label: "Windows",
		icon: AppWindow,
		sections: [
			{ id: "sizes", label: "Window Sizes", icon: AppWindow }
		]
	},
	{
		id: "overlays",
		label: "Overlays",
		icon: Layers,
		sections: [
			{ id: "volume", label: "Volume", icon: Volume2 },
			{ id: "brightness", label: "Brightness", icon: Sun }
		]
	},
	{
		id: "notifications",
		label: "Notifications",
		icon: Bell,
		sections: [
			{ id: "popups", label: "Pop-ups", icon: Bell },
			{ id: "center", label: "Center", icon: PanelRight }
		]
	},
	{
		id: "search-bar",
		label: "Search Bar Pill",
		icon: Search,
		sections: [
			{ id: "general", label: "General", icon: Search },
			{ id: "look", label: "Look", icon: Palette },
			{ id: "engines", label: "Engines", icon: Globe },
			{ id: "plugins", label: "Plugins", icon: Puzzle },
			{ id: "apps", label: "Apps & Files", icon: FolderSearch },
			{ id: "advanced", label: "Advanced", icon: SlidersHorizontal }
		]
	},
	{
		id: "integrations",
		label: "Integrations",
		icon: Blocks,
		sections: [
			{ id: "apps", label: "Apps", icon: Blocks },
			{ id: "blitzit", label: "Blitzit", icon: Zap }
		]
	},
	{ id: "about", label: "About", icon: Info, sections: [] }
];

/** Where the settings window is: a category and one of its sub-sections. */
interface Place {
	category: string;
	section: string;
}

const SECTIONS_KEY = "bloom-settings-sections";

/** Each category's last-open sub-section, to come back to it. */
function readRemembered(): Record<string, string> {
	try {
		const parsed: unknown = JSON.parse(localStorage.getItem(SECTIONS_KEY) ?? "{}");
		return parsed && typeof parsed === "object" ? (parsed as Record<string, string>) : {};
	} catch {
		return {};
	}
}

/**
 * A page asked for by name ("notch", "notch/pages", or an older name such
 * as "integration-blitzit"), with its remembered sub-section when none is
 * named.
 */
function placeFor(tab: string, remembered: Record<string, string>): Place {
	if (tab === "integration-blitzit") return { category: "integrations", section: "blitzit" };
	// Window borders moved from Windows to Appearance.
	if (tab === "windows/borders") return { category: "appearance", section: "borders" };
	const [categoryId, sectionId] = tab.split("/");
	const category = CATEGORIES.find((c) => c.id === categoryId) ?? CATEGORIES[0];
	const has = (id: string | undefined) => !!id && category.sections.some((s) => s.id === id);
	const section = has(sectionId)
		? sectionId
		: has(remembered[category.id])
			? remembered[category.id]
			: (category.sections[0]?.id ?? "");
	return { category: category.id, section };
}

function SettingsApp() {
	const [place, setPlace] = useState<Place>(() => placeFor("general", readRemembered()));
	// Read fresh each time: the page-opening listener below is set up once.
	const go = (tab: string) => {
		const remembered = readRemembered();
		const next = placeFor(tab, remembered);
		setPlace(next);
		if (next.section) {
			try {
				localStorage.setItem(
					SECTIONS_KEY,
					JSON.stringify({ ...remembered, [next.category]: next.section })
				);
			} catch {}
		}
	};
	const { category, section } = place;
	const settings = useSettings();

	useEffect(() => {
		return initTheme();
	}, []);

	useEffect(() => {
		const unlisten = listen<SettingsTab | string>("settings-open-tab", (e) => {
			go(e.payload);
			void invoke("settings_take_tab").catch(() => {});
		});
		// Opened on a page before this window was ready (it is made a moment
		// after Bloom starts): that page.
		void invoke<string | null>("settings_take_tab")
			.then((tab) => {
				if (tab) go(tab);
			})
			.catch(() => {});
		return () => {
			void unlisten.then((fn) => fn());
		};
	}, []);

	useEffect(() => {
		const preventContext = (e: MouseEvent) => e.preventDefault();
		document.addEventListener("contextmenu", preventContext as any);

		appWindow
			.setEffects({
				effects: ["mica" as Effect],
				state: "active" as any
			})
			.catch(() => {});

		return () => {
			document.removeEventListener("contextmenu", preventContext as any);
		};
	}, []);

	// The settings window follows the notch's size.
	useEffect(() => {
		invoke("resize_settings_window", {
			width: 620 * settings.notchScale,
			height: 480 * settings.notchScale
		}).catch(console.error);
	}, [settings.notchScale]);

	const handleClose = async (e: React.MouseEvent) => {
		e.preventDefault();
		e.stopPropagation();
		try {
			await appWindow.hide();
		} catch {}
	};

	return (
		<div className="settings-container" style={{ zoom: settings.notchScale }}>
			<div className="title-bar" data-tauri-drag-region>
				<span className="title-text" data-tauri-drag-region>
					Settings
				</span>
				<button className="close-btn" onClick={handleClose} title="Close Settings">
					<X size={12} strokeWidth={1.5} className="close-btn-icon" />
				</button>
			</div>

			<div className="settings-body">
				<div className="settings-sidebar">
					{CATEGORIES.map(({ id, label, icon: CategoryIcon, sections }) => (
						<div key={id} className="sidebar-tab-group">
							<button
								className={`sidebar-tab ${category === id ? "active" : ""}`}
								aria-expanded={sections.length > 0 ? category === id : undefined}
								onClick={() => go(id)}
							>
								<div className="sidebar-tab-icon">
									<CategoryIcon size={14} strokeWidth={1.5} />
								</div>
								<span>{label}</span>
							</button>
							{category === id &&
								sections.map(({ id: sectionId, label: sectionLabel, icon: SectionIcon }) => (
									<button
										key={sectionId}
										className={`sidebar-tab sidebar-subtab ${section === sectionId ? "active" : ""}`}
										onClick={() => go(`${id}/${sectionId}`)}
									>
										<div className="sidebar-tab-icon">
											<SectionIcon size={12} strokeWidth={1.5} />
										</div>
										<span>{sectionLabel}</span>
									</button>
								))}
						</div>
					))}
				</div>

				<div className="settings-content">
					{category === "general" && (
						<GeneralTab
							section={section as GeneralSection}
							autostart={settings.autostart}
							toggleAutostart={settings.toggleAutostart}
							timeFormat24h={settings.timeFormat24h}
							toggleTimeFormat24h={settings.toggleTimeFormat24h}
							lowBatteryThreshold={settings.lowBatteryThreshold}
							handleThresholdChange={settings.handleThresholdChange}
							restartBloom={settings.restartBloom}
							resetToDefaults={settings.resetToDefaults}
							quitBloom={settings.quitBloom}
						/>
					)}
					{category === "appearance" && (
						<AppearanceTab
							section={section as AppearanceSection}
							themeMode={settings.themeMode}
							handleThemeModeChange={settings.handleThemeModeChange}
							themeColor={settings.themeColor}
							handleThemeColorChange={settings.handleThemeColorChange}
							themeOpacity={settings.themeOpacity}
							handleOpacityChange={settings.handleOpacityChange}
							themeSaturation={settings.themeSaturation}
							handleSaturationChange={settings.handleSaturationChange}
							themeBrightness={settings.themeBrightness}
							handleBrightnessChange={settings.handleBrightnessChange}
							cornersEnabled={settings.cornersEnabled}
							toggleCorners={settings.toggleCorners}
							notchScale={settings.notchScale}
							handleNotchScaleChange={settings.handleNotchScaleChange}
							dockScale={settings.dockScale}
							handleDockScaleChange={settings.handleDockScaleChange}
						/>
					)}
					{category === "notch" && (
						<NotchTab
							section={section as NotchSection}
							notchMode={settings.notchMode}
							setNotchModeValue={settings.setNotchModeValue}
							notchEdgeDelay={settings.notchEdgeDelay}
							handleNotchEdgeDelayChange={settings.handleNotchEdgeDelayChange}
							calendarEnabled={settings.calendarEnabled}
							timerSoundEnabled={settings.timerSoundEnabled}
							toggleTimerSound={settings.toggleTimerSound}
							musicModeEnabled={settings.musicModeEnabled}
							toggleMusicMode={settings.toggleMusicMode}
							musicCompactNotch={settings.musicCompactNotch}
							toggleMusicCompactNotch={settings.toggleMusicCompactNotch}
							notchWidgetsOnHover={settings.notchWidgetsOnHover}
							toggleNotchWidgetsOnHover={settings.toggleNotchWidgetsOnHover}
							shelfPersist={settings.shelfPersist}
							toggleShelfPersist={settings.toggleShelfPersist}
							mediaLayout={settings.mediaLayout}
							toggleMediaLayout={settings.toggleMediaLayout}
							mediaAmbienceEnabled={settings.mediaAmbienceEnabled}
							toggleAmbience={settings.toggleAmbience}
							mediaCompactGlowEnabled={settings.mediaCompactGlowEnabled}
							toggleCompactGlow={settings.toggleCompactGlow}
							weatherEnabled={settings.weatherEnabled}
							toggleWeather={settings.toggleWeather}
							tempUnitFahrenheit={settings.tempUnitFahrenheit}
							toggleTempUnit={settings.toggleTempUnit}
							cityName={settings.cityName}
							setCityName={settings.setCityName}
							citySearchResults={settings.citySearchResults}
							showCityDropdown={settings.showCityDropdown}
							setShowCityDropdown={settings.setShowCityDropdown}
							selectCity={settings.selectCity}
							handleCityClear={settings.handleCityClear}
							statusWidgets={settings.statusWidgets}
							handleWidgetsChange={settings.handleWidgetsChange}
						/>
					)}
					{category === "dock" && (
						<DockTab
							section={section as DockSection}
							dockEnabled={settings.dockEnabled}
							toggleDock={settings.toggleDock}
							dockMode={settings.dockMode}
							setDockModeValue={settings.setDockModeValue}
							dockEdgeDelay={settings.dockEdgeDelay}
							handleDockEdgeDelayChange={settings.handleDockEdgeDelayChange}
							dockPreviewEnabled={settings.dockPreviewEnabled}
							toggleDockPreview={settings.toggleDockPreview}
							dockIconOnly={settings.dockIconOnly}
							toggleDockIconOnly={settings.toggleDockIconOnly}
							dockSeparatorEnabled={settings.dockSeparatorEnabled}
							toggleDockSeparator={settings.toggleDockSeparator}
							dockAdaptive={settings.dockAdaptive}
							toggleDockAdaptive={settings.toggleDockAdaptive}
							dockWinNumberEnabled={settings.dockWinNumberEnabled}
							toggleDockWinNumber={settings.toggleDockWinNumber}
							dockExtrasPosition={settings.dockExtrasPosition}
							handleDockExtrasPositionChange={settings.handleDockExtrasPositionChange}
							dockExtrasDrives={settings.dockExtrasDrives}
							toggleDockExtrasDrives={settings.toggleDockExtrasDrives}
							dockExtrasRecycleBin={settings.dockExtrasRecycleBin}
							toggleDockExtrasRecycleBin={settings.toggleDockExtrasRecycleBin}
							dockExtrasFolders={settings.dockExtrasFolders}
							toggleDockExtraFolder={settings.toggleDockExtraFolder}
							dockExtrasCustomFolders={settings.dockExtrasCustomFolders}
							handleAddDockExtraFolder={settings.handleAddDockExtraFolder}
							handleRemoveDockExtraFolder={settings.handleRemoveDockExtraFolder}
							startIcon={settings.startIcon}
							handleStartIconChange={settings.handleStartIconChange}
						/>
					)}
					{category === "windows" && <WindowsTab section={section as WindowsSection} />}
					{category === "overlays" && (
						<OverlaysTab
							section={section as OverlaysSection}
							volumeOverlayEnabled={settings.volumeOverlayEnabled}
							toggleVolumeOverlay={settings.toggleVolumeOverlay}
							volumeEdgeEnabled={settings.volumeEdgeEnabled}
							toggleVolumeEdge={settings.toggleVolumeEdge}
							brightnessOverlayEnabled={settings.brightnessOverlayEnabled}
							toggleBrightnessOverlay={settings.toggleBrightnessOverlay}
							brightnessEdgeEnabled={settings.brightnessEdgeEnabled}
							toggleBrightnessEdge={settings.toggleBrightnessEdge}
						/>
					)}
					{category === "notifications" && (
						<NotificationsTab section={section as NotificationsSection} />
					)}
					{category === "search-bar" && <SearchBarTab section={section as SearchBarSection} />}
					{category === "integrations" && section === "apps" && <IntegrationsTab openTab={go} />}
					{category === "integrations" && section === "blitzit" && <BlitzitSettings />}
					{category === "about" && (
						<AboutTab
							appVersion={settings.appVersion}
							exportStatus={settings.exportStatus}
							importStatus={settings.importStatus}
							handleExportSettings={settings.handleExportSettings}
							handleImportSettings={settings.handleImportSettings}
						/>
					)}
				</div>
			</div>
		</div>
	);
}

createRoot(document.getElementById("root") as HTMLElement).render(
	<StrictMode>
		<SettingsApp />
	</StrictMode>
);
