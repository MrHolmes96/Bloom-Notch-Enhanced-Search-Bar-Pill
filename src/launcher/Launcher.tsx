import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { Search } from "lucide-react";
import { initTheme } from "../theme";
import { useSettingsSync } from "../hooks/useSettingsSync";
import type { AppInfo } from "../dockApps";
import { CONFIG_KEY, loadConfig, parseConfig, type LauncherConfig } from "./config";
import {
	expandShortcuts,
	historyResults,
	homeResults,
	keywordTargets,
	makeContext,
	parseQuery,
	runFallback,
	runSearch,
	sortResults
} from "./engine";
import { boost, queryHistory, recentHistory, recordUse } from "./history";
import { Glyph, ResultIcon, clearIconCache } from "./icons";
import { copy } from "./ops";
import { clearBookmarkCache } from "./plugins/web";
import { clearProcessCache } from "./plugins/shell";
import { Preview } from "./Preview";
import type { DirEntry, Result, ResultAction, SearchContext } from "./types";
import "./Launcher.css";

const appWindow = getCurrentWebviewWindow();
const ROW_HEIGHT = 50;
/** Pause in typing before slow searches (files, suggestions) start. */
const SEARCH_DELAY = 120;

interface Menu {
	result: Result;
	actions: ResultAction[];
	selected: number;
}

function useClock(enabled: boolean) {
	const [now, setNow] = useState(() => new Date());
	useEffect(() => {
		if (!enabled) return;
		const timer = setInterval(() => setNow(new Date()), 15_000);
		return () => clearInterval(timer);
	}, [enabled]);
	const h24 = localStorage.getItem("bloom-time-format-24h") === "true";
	return now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: !h24 });
}

/** The search bar's own colors (Settings > Search Bar Pill > Look). */
function lookStyle(look: LauncherConfig["appearance"]): React.CSSProperties {
	const opacity = Math.max(0.2, Math.min(1, look.opacity));
	if (look.theme !== "custom") {
		// Bloom's colors with the search bar's own opacity.
		return {
			"--launcher-bg": `rgb(from var(--bloom-bg) r g b / ${opacity})`,
			"--launcher-bg-expanded": `rgb(from var(--bloom-bg-expanded) r g b / ${Math.min(1, opacity + 0.03)})`
		} as React.CSSProperties;
	}
	const hex = /^#?([0-9a-f]{6})$/i.exec(look.color.trim())?.[1] ?? "1c1c1e";
	const r = parseInt(hex.slice(0, 2), 16);
	const g = parseInt(hex.slice(2, 4), 16);
	const b = parseInt(hex.slice(4, 6), 16);
	const light = (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.55;
	return {
		"--launcher-bg": `rgba(${r}, ${g}, ${b}, ${opacity})`,
		"--launcher-bg-expanded": `rgba(${r}, ${g}, ${b}, ${Math.min(1, opacity + 0.03)})`,
		"--bloom-text": light ? "#1c1c1e" : "#ffffff",
		"--bloom-text-muted": light ? "rgba(28, 28, 30, 0.6)" : "rgba(255, 255, 255, 0.6)",
		"--bloom-border": light ? "rgba(0, 0, 0, 0.12)" : "rgba(255, 255, 255, 0.12)",
		"--bloom-group-bg": light ? "rgba(0, 0, 0, 0.05)" : "rgba(255, 255, 255, 0.08)"
	} as React.CSSProperties;
}

export default function Launcher() {
	const [config, setConfig] = useState<LauncherConfig>(loadConfig);
	const [query, setQueryState] = useState("");
	const [results, setResults] = useState<Result[]>([]);
	const [selected, setSelected] = useState(0);
	const [menu, setMenu] = useState<Menu | null>(null);
	const [confirmKey, setConfirmKey] = useState<string | null>(null);
	const [message, setMessage] = useState<string | null>(null);
	const [previewOpen, setPreviewOpen] = useState(config.alwaysPreview);
	const [historyMode, setHistoryMode] = useState(false);
	const [apps, setApps] = useState<AppInfo[]>([]);
	const [pathPrograms, setPathPrograms] = useState<DirEntry[]>([]);
	const [tick, setTick] = useState(0);

	const inputRef = useRef<HTMLInputElement>(null);
	const shellRef = useRef<HTMLDivElement>(null);
	const listRef = useRef<HTMLDivElement>(null);
	const searchId = useRef(0);
	const navigated = useRef(false);
	const queryCycle = useRef(-1);
	const messageTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
	const configRef = useRef(config);
	configRef.current = config;
	const selectedRef = useRef(selected);
	selectedRef.current = selected;
	const resultsRef = useRef<Result[]>([]);

	useEffect(() => initTheme(), []);

	useSettingsSync({
		[CONFIG_KEY]: (value) => setConfig(parseConfig(value))
	});

	const setQuery = useCallback((q: string) => {
		setQueryState(q);
		navigated.current = false;
		setMenu(null);
		setConfirmKey(null);
		requestAnimationFrame(() => {
			const input = inputRef.current;
			if (input) {
				input.focus();
				input.setSelectionRange(q.length, q.length);
			}
		});
	}, []);

	const notify = useCallback((text: string) => {
		setMessage(text);
		if (messageTimer.current) clearTimeout(messageTimer.current);
		messageTimer.current = setTimeout(() => setMessage(null), 3500);
	}, []);

	const loadApps = useCallback(() => {
		invoke<AppInfo[]>("get_installed_apps")
			.then((list) => setApps(list.filter((a) => a.name && a.path)))
			.catch(console.error);
		if (configRef.current.programs.pathSource) {
			invoke<DirEntry[]>("launcher_path_programs")
				.then(setPathPrograms)
				.catch(() => setPathPrograms([]));
		}
	}, []);

	const hide = useCallback(() => {
		void appWindow.hide();
	}, []);

	const reload = useCallback(() => {
		clearBookmarkCache();
		clearProcessCache();
		clearIconCache();
		void invoke("launcher_reindex").catch(() => {});
		setTimeout(loadApps, 1500);
		loadApps();
		setTick((t) => t + 1);
	}, [loadApps]);

	const openSettings = useCallback(() => {
		hide();
		void invoke("launcher_open_settings");
	}, [hide]);

	const ctx: SearchContext = useMemo(
		() =>
			makeContext(
				{
					config,
					apps,
					pathPrograms,
					setQuery,
					requery: () => setTick((t) => t + 1),
					notify,
					reload,
					openSettings
				},
				boost
			),
		[config, apps, pathPrograms, setQuery, notify, reload, openSettings]
	);

	// Opening: apply "last query", refresh apps, focus.
	useEffect(() => {
		loadApps();
		const unlistenShow = listen<{ query: string | null }>("launcher-show", (event) => {
			const preset = event.payload?.query;
			const mode = configRef.current.lastQuery;
			setMenu(null);
			setConfirmKey(null);
			setHistoryMode(false);
			setMessage(null);
			setPreviewOpen(configRef.current.alwaysPreview);
			queryCycle.current = -1;
			navigated.current = false;
			if (configRef.current.animation) {
				shellRef.current?.animate(
					[
						{ opacity: 0, transform: "translateY(-6px) scale(0.98)" },
						{ opacity: 1, transform: "none" }
					],
					{ duration: 140, easing: "cubic-bezier(0.2, 0.9, 0.3, 1)" }
				);
			}
			if (preset != null) {
				setQueryState(preset);
			} else if (mode === "empty") {
				setQueryState("");
			}
			setTick((t) => t + 1);
			loadApps();
			requestAnimationFrame(() => {
				const input = inputRef.current;
				if (!input) return;
				input.focus();
				if (preset == null && mode === "selected") input.select();
				else input.setSelectionRange(input.value.length, input.value.length);
			});
		});
		const unlistenFocus = appWindow.onFocusChanged(({ payload: focused }) => {
			if (!focused && configRef.current.hideOnBlur) void appWindow.hide();
		});
		return () => {
			unlistenShow.then((f) => f());
			unlistenFocus.then((f) => f());
		};
	}, [loadApps]);

	const targets = useMemo(() => keywordTargets(config), [config]);
	const parsed = useMemo(
		() => parseQuery(expandShortcuts(query, config, null), targets),
		[query, config, targets]
	);

	// Searching.
	useEffect(() => {
		const id = ++searchId.current;
		const isCurrent = () => searchId.current === id;
		const apply = (list: Result[]) => {
			if (!isCurrent()) return;
			const sorted = sortResults(list);
			if (navigated.current) {
				const key = resultsRef.current[selectedRef.current]?.key;
				const at = sorted.findIndex((r) => r.key === key);
				setSelected(at >= 0 ? at : 0);
			} else {
				setSelected(0);
			}
			resultsRef.current = sorted;
			setResults(sorted);
		};

		if (historyMode) {
			apply(historyResults(ctx, recentHistory(), query));
			return;
		}
		if (!query.trim()) {
			apply(homeResults(ctx));
			return;
		}

		let timer: ReturnType<typeof setTimeout> | null = null;
		const start = async () => {
			let text = query;
			if (/\{clipboard\}/i.test(text)) {
				const clip = await readText().catch(() => "");
				if (!isCurrent()) return;
				text = expandShortcuts(text, config, clip);
			} else {
				text = expandShortcuts(text, config, null);
			}
			const q = parseQuery(text, targets);
			let run = runSearch(ctx, q, targets, isCurrent);
			if (run.keywordEmpty && run.now.length === 0 && run.later.length === 0) {
				run = { ...runFallback(ctx, q, isCurrent), keywordEmpty: false };
			}
			let collected = [...run.now];
			apply(collected);
			if (!run.later.length) return;
			timer = setTimeout(() => {
				for (const load of run.later) {
					load()
						.then((more) => {
							if (!isCurrent() || !more.length) return;
							collected = [...collected, ...more];
							apply(collected);
						})
						.catch(() => {});
				}
			}, SEARCH_DELAY);
		};
		void start();
		return () => {
			if (timer) clearTimeout(timer);
		};
	}, [query, ctx, targets, historyMode, tick, config]);

	const rows: { key: string; title: string; subtitle?: string; icon: Result["icon"] }[] = menu
		? menu.actions.map((a) => ({
				key: a.key,
				title: a.title,
				subtitle: a.subtitle,
				icon: { kind: "glyph" as const, glyph: a.glyph }
			}))
		: results;
	const current = menu ? menu.selected : selected;
	const selectedResult = menu ? menu.result : results[selected];

	// Keep the selected row in view.
	useEffect(() => {
		const list = listRef.current;
		if (!list) return;
		const top = current * ROW_HEIGHT;
		if (top < list.scrollTop) list.scrollTop = top;
		else if (top + ROW_HEIGHT > list.scrollTop + list.clientHeight) {
			list.scrollTop = top + ROW_HEIGHT - list.clientHeight;
		}
	}, [current, rows.length]);

	// The window is exactly as big as what it shows, so the space around the
	// pill never blocks clicks underneath.
	const showPreview = previewOpen && !menu && !!selectedResult?.preview;
	useLayoutEffect(() => {
		const el = shellRef.current;
		if (!el) return;
		const resize = () => {
			const height = Math.ceil(el.getBoundingClientRect().height);
			const width = configRef.current.width + 24 + (showPreview ? 280 : 0);
			void invoke("launcher_set_size", { width, height: height + 12 }).catch(() => {});
		};
		resize();
		const observer = new ResizeObserver(resize);
		observer.observe(el);
		return () => observer.disconnect();
	}, [config.width, showPreview]);

	const execute = useCallback(
		async (result: Result, how: "run" | "admin" | "location" = "run") => {
			const fn =
				how === "admin" ? result.runAsAdmin : how === "location" ? result.openLocation : result.run;
			if (!fn) return;
			if (how === "run" && result.confirm && confirmKey !== result.key) {
				setConfirmKey(result.key);
				return;
			}
			setConfirmKey(null);
			recordUse(result, query);
			const stay = how === "run" && result.keepOpen;
			if (!stay) hide();
			try {
				await fn();
			} catch (e) {
				if (stay) notify(String(e));
				else console.error(e);
			}
		},
		[confirmKey, hide, notify, query]
	);

	const runAction = useCallback(
		async (action: ResultAction, result: Result) => {
			const key = `${result.key}#${action.key}`;
			if (action.confirm && confirmKey !== key) {
				setConfirmKey(key);
				return;
			}
			setConfirmKey(null);
			if (!action.keepOpen) hide();
			else setMenu(null);
			try {
				await action.run();
			} catch (e) {
				if (action.keepOpen) notify(String(e));
			}
		},
		[confirmKey, hide, notify]
	);

	const openMenu = useCallback(async (result: Result | undefined) => {
		if (!result?.actions) return;
		const actions = await result.actions();
		if (!actions.length) return;
		setConfirmKey(null);
		setMenu({ result, actions, selected: 0 });
	}, []);

	const move = useCallback(
		(delta: number) => {
			setConfirmKey(null);
			navigated.current = true;
			if (menu) {
				const n = menu.actions.length;
				setMenu({ ...menu, selected: (menu.selected + delta + n) % n });
			} else if (results.length) {
				setSelected((s) => Math.min(Math.max(s + delta, 0), results.length - 1));
			}
		},
		[menu, results.length]
	);

	const cycleQuery = (direction: number) => {
		const past = queryHistory();
		if (!past.length) return;
		const next = Math.min(Math.max(queryCycle.current + direction, -1), past.length - 1);
		queryCycle.current = next;
		setQuery(next < 0 ? "" : past[next]);
	};

	const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
		const input = e.currentTarget;
		const page = Math.max(config.maxResults - 1, 1);
		if (e.key === "Escape") {
			e.preventDefault();
			if (menu) setMenu(null);
			else if (historyMode) setHistoryMode(false);
			else hide();
			return;
		}
		if (e.key === "Enter") {
			e.preventDefault();
			if (menu) {
				const action = menu.actions[menu.selected];
				if (action) void runAction(action, menu.result);
				return;
			}
			const result = results[selected];
			if (!result) return;
			if (e.shiftKey && !e.ctrlKey) void openMenu(result);
			else if (e.ctrlKey && e.shiftKey) void execute(result, "admin");
			else if (e.ctrlKey) void execute(result, "location");
			else void execute(result);
			return;
		}
		if (e.key === "ArrowDown" || (e.key === "Tab" && !e.shiftKey && !e.ctrlKey)) {
			e.preventDefault();
			if (e.altKey && e.key === "ArrowDown") return cycleQuery(-1);
			if (!menu && results.length && selected === results.length - 1 && e.key === "Tab") {
				navigated.current = true;
				return setSelected(0);
			}
			return move(1);
		}
		if (e.key === "ArrowUp" || (e.key === "Tab" && e.shiftKey && !e.ctrlKey)) {
			e.preventDefault();
			if (e.altKey && e.key === "ArrowUp") return cycleQuery(1);
			return move(-1);
		}
		if (e.key === "PageDown") {
			e.preventDefault();
			return move(page);
		}
		if (e.key === "PageUp") {
			e.preventDefault();
			return move(-page);
		}
		if (e.key === "Tab" && e.ctrlKey) {
			e.preventDefault();
			const r = results[selected];
			if (r) {
				const prefix = parsed.keyword && r.plugin !== "explorer" ? `${parsed.keyword} ` : "";
				setQuery(r.autoComplete ? prefix + r.autoComplete : prefix + r.title);
			}
			return;
		}
		if ((e.key === "o" || e.key === "O") && e.ctrlKey) {
			e.preventDefault();
			if (menu) setMenu(null);
			else void openMenu(results[selected]);
			return;
		}
		if (
			e.key === "ArrowRight" &&
			!menu &&
			input.selectionStart === input.value.length &&
			e.ctrlKey
		) {
			e.preventDefault();
			void openMenu(results[selected]);
			return;
		}
		if (e.key === "ArrowLeft" && menu && input.selectionStart === 0) {
			e.preventDefault();
			setMenu(null);
			return;
		}
		if ((e.key === "h" || e.key === "H") && e.ctrlKey) {
			e.preventDefault();
			setHistoryMode((h) => !h);
			navigated.current = false;
			return;
		}
		if ((e.key === "i" || e.key === "I") && e.ctrlKey) {
			e.preventDefault();
			openSettings();
			return;
		}
		if (e.key === "F5") {
			e.preventDefault();
			reload();
			notify("Search data reloaded");
			return;
		}
		if (e.key === "F1") {
			e.preventDefault();
			setPreviewOpen((p) => !p);
			return;
		}
		if (
			(e.key === "c" || e.key === "C") &&
			e.ctrlKey &&
			input.selectionStart === input.selectionEnd
		) {
			const r = menu ? menu.result : results[selected];
			if (r?.copyText) {
				e.preventDefault();
				void copy(r.copyText).then(() => notify("Copied"));
			}
			return;
		}
		if (e.altKey && /^Digit[1-9]$/.test(e.code)) {
			e.preventDefault();
			const list = listRef.current;
			const first = list ? Math.floor(list.scrollTop / ROW_HEIGHT) : 0;
			const index = first + Number(e.code.slice(5)) - 1;
			if (menu) {
				const action = menu.actions[index];
				if (action) void runAction(action, menu.result);
			} else if (results[index]) {
				void execute(results[index]);
			}
		}
	};

	const clock = useClock(config.showClock);
	const placeholder =
		config.placeholder.trim() ||
		(historyMode ? "Search your history" : "Search apps, files, settings and the web");

	const visibleRows = Math.min(rows.length, Math.max(config.maxResults, 1));
	const confirmFor = (key: string) =>
		menu ? confirmKey === `${menu.result.key}#${key}` : confirmKey === key;
	const confirmText = (index: number) => {
		if (menu) return menu.actions[index]?.confirm;
		return results[index]?.confirm;
	};

	return (
		<div
			className="launcher-shell"
			ref={shellRef}
			style={{
				width: config.width + 24 + (showPreview ? 280 : 0),
				...lookStyle(config.appearance)
			}}
		>
			<div className="launcher-pill">
				<Search size={18} className="launcher-search-icon" />
				{menu && (
					<span className="launcher-chip" title={menu.result.title}>
						{menu.result.title}
					</span>
				)}
				{historyMode && !menu && <span className="launcher-chip">History</span>}
				<input
					ref={inputRef}
					className="launcher-input"
					value={query}
					onChange={(e) => {
						queryCycle.current = -1;
						setQuery(e.target.value);
					}}
					onKeyDown={onKeyDown}
					placeholder={placeholder}
					spellCheck={false}
					autoFocus
				/>
				{config.showClock && !query && <span className="launcher-clock">{clock}</span>}
			</div>
			{(rows.length > 0 || message) && (
				<div className="launcher-panel">
					<div className="launcher-results-column">
						{message && <div className="launcher-message">{message}</div>}
						{rows.length > 0 && (
							<div
								className="launcher-results"
								role="listbox"
								ref={listRef}
								style={{ maxHeight: visibleRows * ROW_HEIGHT }}
							>
								{rows.map((r, i) => {
									const pendingConfirm = confirmFor(r.key) ? confirmText(i) : undefined;
									return (
										<div
											key={r.key}
											role="option"
											aria-selected={i === current}
											className={`launcher-row ${i === current ? "selected" : ""} ${pendingConfirm ? "confirm" : ""}`}
											style={{ height: ROW_HEIGHT }}
											onMouseMove={() => {
												if (i === current) return;
												navigated.current = true;
												setConfirmKey(null);
												if (menu) setMenu({ ...menu, selected: i });
												else setSelected(i);
											}}
											onClick={() => {
												if (menu) void runAction(menu.actions[i], menu.result);
												else void execute(results[i]);
											}}
											onContextMenu={(e) => {
												e.preventDefault();
												if (!menu) {
													setSelected(i);
													void openMenu(results[i]);
												}
											}}
										>
											<div className="launcher-row-icon">
												<ResultIcon icon={r.icon} />
											</div>
											<div className="launcher-row-text">
												<span className="launcher-row-title">{r.title}</span>
												{(pendingConfirm || r.subtitle) && (
													<span className="launcher-row-subtitle">
														{pendingConfirm ?? r.subtitle}
													</span>
												)}
											</div>
											{!menu && i === current && results[i]?.actions && (
												<span className="launcher-row-hint" title="More actions (Shift+Enter)">
													<Glyph name="properties" size={14} />
												</span>
											)}
										</div>
									);
								})}
							</div>
						)}
					</div>
					{showPreview && selectedResult?.preview && (
						<Preview preview={selectedResult.preview} title={selectedResult.title} />
					)}
				</div>
			)}
		</div>
	);
}
