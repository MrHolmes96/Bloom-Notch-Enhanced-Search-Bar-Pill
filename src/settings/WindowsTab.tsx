import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AppWindow, ChevronDown, ChevronRight, Eraser, LayoutGrid } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { AppImage } from "./TraySettings";
import { useStoredFlag } from "./useStoredSetting";

/** Settings > Windows' sub-sections. */
export type WindowsSection = "sizes";

type Mode = "all" | "main" | "off";

interface WindowEntry {
	id: string;
	title: string;
	/** The entry shared by the app's windows whose titles change. */
	shared: boolean;
	main: boolean;
	size: string | null;
}

interface AppEntry {
	path: string;
	name: string;
	mode: Mode;
	windows: WindowEntry[];
}

function summary(app: AppEntry) {
	if (app.mode === "off") return "Off: Bloom leaves its windows alone";
	const saved = app.windows.filter((w) => w.size).length;
	if (app.mode === "main") {
		const mains = app.windows.filter((w) => w.main).map((w) => w.title || "Untitled window");
		return mains.length ? `Main window only: ${mains.join(", ")}` : "Main window only: none ticked yet";
	}
	return `${saved} window${saved === 1 ? "" : "s"} remembered`;
}

function windowLabel(w: WindowEntry, app: AppEntry) {
	if (!w.shared) return w.title;
	const named = app.windows.some((o) => !o.shared);
	if (!w.title) return named ? "Its other windows" : "Its windows";
	return named ? `Its other windows, like “${w.title}”` : `Its windows, like “${w.title}”`;
}

/** How Bloom treats other apps' windows. */
export function WindowsTab(_props: { section: WindowsSection }) {
	const [windowMemory, setWindowMemory] = useStoredFlag("bloom-window-memory", true);
	const [forgotten, setForgotten] = useState(false);
	const [apps, setApps] = useState<AppEntry[] | null>(null);
	const [open, setOpen] = useState<string | null>(null);

	const reload = useCallback(() => {
		void invoke<AppEntry[]>("window_memory_apps")
			.then(setApps)
			.catch(() => setApps([]));
	}, []);

	useEffect(() => {
		if (windowMemory) reload();
	}, [windowMemory, reload]);

	const setMode = (app: AppEntry, mode: Mode) => {
		setApps((list) => list?.map((a) => (a.path === app.path ? { ...a, mode } : a)) ?? list);
		void invoke("window_memory_set_mode", { path: app.path, mode }).finally(reload);
		if (mode === "main") setOpen(app.path);
	};

	const setMain = (app: AppEntry, w: WindowEntry, main: boolean) => {
		setApps(
			(list) =>
				list?.map((a) =>
					a.path === app.path
						? { ...a, windows: a.windows.map((o) => (o.id === w.id ? { ...o, main } : o)) }
						: a
				) ?? list
		);
		void invoke("window_memory_set_main", { path: app.path, id: w.id, main }).finally(reload);
	};

	return (
		<>
			<div className="setting-group-label">Window Sizes</div>
			<div className="setting-group">
				<SettingRow
					icon={AppWindow}
					label="Remember Window Sizes"
					desc="Apps open at the size and place you last left them. Only your own moves and resizes are saved, never an app sizing itself"
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
						desc={forgotten ? "Forgotten" : "Let every app choose its own size again (each app's setting below stays)"}
						action
						divider={false}
						onClick={() => {
							void invoke("forget_window_sizes").finally(reload);
							setForgotten(true);
							setTimeout(() => setForgotten(false), 2500);
						}}
					/>
				)}
			</div>

			{windowMemory && (
				<>
					<div className="setting-group-label">Apps</div>
					<div className="setting-group">
						{apps !== null && apps.length === 0 && (
							<SettingRow
								icon={LayoutGrid}
								label="No Apps Yet"
								desc="Apps show here once you've moved or resized one of their windows"
								divider={false}
							/>
						)}
						{apps?.map((app, i) => {
							const expanded = open === app.path;
							const last = i === apps.length - 1;
							return (
								<div key={app.path}>
									<div
										className="setting-item action"
										onClick={() => setOpen(expanded ? null : app.path)}
									>
										<div className="setting-icon-bg">
											<AppImage path={app.path} />
										</div>
										<div className="setting-info">
											<span className="setting-label">{app.name}</span>
											<span className="setting-desc">{summary(app)}</span>
										</div>
										<select
											className="settings-select"
											value={app.mode}
											onClick={(e) => e.stopPropagation()}
											onChange={(e) => setMode(app, e.target.value as Mode)}
										>
											<option value="all">All windows</option>
											<option value="main">Main window only</option>
											<option value="off">Off</option>
										</select>
										{expanded ? (
											<ChevronDown size={14} strokeWidth={1.5} style={{ marginLeft: 8, opacity: 0.6 }} />
										) : (
											<ChevronRight size={14} strokeWidth={1.5} style={{ marginLeft: 8, opacity: 0.6 }} />
										)}
									</div>
									{expanded && (
										<>
											<div className="setting-divider" />
											{app.mode === "main" && (
												<div className="setting-item" style={{ paddingLeft: 54 }}>
													<div className="setting-info">
														<span className="setting-desc">
															Tick the windows Bloom should remember. The others open as {app.name} sizes them.
														</span>
													</div>
												</div>
											)}
											{app.windows.map((w) => (
												<div key={w.id}>
													<div className="setting-item" style={{ paddingLeft: 54 }}>
														<div className="setting-info">
															<span className="setting-label">{windowLabel(w, app)}</span>
															<span className="setting-desc">
																{app.mode === "off"
																	? "Left alone"
																	: app.mode === "main" && !w.main
																		? "Not remembered: opens as the app sizes it"
																		: (w.size ?? "Nothing saved yet")}
															</span>
														</div>
														{app.mode === "main" && (
															<label className="toggle-switch">
																<input
																	type="checkbox"
																	checked={w.main}
																	onChange={(e) => setMain(app, w, e.target.checked)}
																/>
																<span className="slider"></span>
															</label>
														)}
													</div>
													<div className="setting-divider" />
												</div>
											))}
											<div
												className="setting-item action"
												style={{ paddingLeft: 54 }}
												onClick={() => {
													void invoke("window_memory_forget_app", { path: app.path }).finally(reload);
												}}
											>
												<div className="setting-info">
													<span className="setting-label">Forget {app.name}'s Sizes</span>
													<span className="setting-desc">Its windows open as the app sizes them until you move them again</span>
												</div>
											</div>
										</>
									)}
									{!last && <div className="setting-divider" />}
								</div>
							);
						})}
					</div>
				</>
			)}
		</>
	);
}
