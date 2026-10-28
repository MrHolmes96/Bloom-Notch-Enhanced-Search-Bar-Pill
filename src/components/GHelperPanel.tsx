import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
	BatteryCharging,
	Bike,
	Car,
	ExternalLink,
	Fan,
	Flower2,
	Gamepad2,
	Gauge,
	Gpu,
	Laptop,
	Leaf,
	Loader2,
	Rocket,
	Settings2,
	Sparkles
} from "lucide-react";
import trayEco from "../assets/ghelper/tray-eco.png";
import trayStandard from "../assets/ghelper/tray-standard.png";
import trayUltimate from "../assets/ghelper/tray-ultimate.png";
import trayLightEco from "../assets/ghelper/tray-light-eco.png";
import trayDarkEco from "../assets/ghelper/tray-dark-eco.png";
import trayLightStandard from "../assets/ghelper/tray-light-standard.png";
import trayDarkStandard from "../assets/ghelper/tray-dark-standard.png";

/** G-Helper's own tray icons (from G-Helper), by the name Bloom gets for the
 *  one G-Helper shows now (`tray_icon`). */
const TRAY_ICONS: Record<string, string> = {
	eco: trayEco,
	standard: trayStandard,
	ultimate: trayUltimate,
	"light-eco": trayLightEco,
	"dark-eco": trayDarkEco,
	"light-standard": trayLightStandard,
	"dark-standard": trayDarkStandard
};
import "./GHelperPanel.css";

/**
 * G-Helper in the notch: its modes, GPU modes, screen and battery charge
 * limit, laid out and coloured as in G-Helper's own window. G-Helper does the
 * work; Bloom asks it through its shortcuts and settings (see ghelper.rs).
 */

export interface GHelperMode {
	id: number;
	name: string;
	base: number;
}

export interface GHelperBinding {
	key: string;
	label: string;
	action: string;
	custom: string;
}

export interface GHelperState {
	installed: boolean;
	running: boolean;
	modes: GHelperMode[];
	mode: number;
	gpu_mode: number;
	/** The icon G-Helper shows in the tray now (see TRAY_ICONS). */
	tray_icon?: string;
	gpu_auto: boolean;
	has_ultimate: boolean;
	screen_auto: boolean;
	refresh: number;
	refresh_min: number;
	refresh_max: number;
	miniled: number | null;
	charge_limit: number;
	bindings: GHelperBinding[];
	shortcuts_off: boolean;
	sensors: {
		/** G-Helper's own lines, as its window shows them. */
		cpu_text: string | null;
		gpu_text: string | null;
		cpu_temp: number | null;
		gpu_temp: number | null;
		cpu_fan: number | null;
		gpu_fan: number | null;
		mid_fan: number | null;
	};
	applying: boolean;
}

// G-Helper's own colours (RForm.cs).
export const GH_COLORS = {
	eco: "#06b48a",
	standard: "#3aaeef",
	turbo: "#ff2020",
	custom: "#ff8000",
	gray: "#a8a8a8"
};

const SILENT = 2;
const BALANCED = 0;
const TURBO = 1;
const ECO = 0;
const STANDARD = 1;
const ULTIMATE = 2;

/** A mode's colour: its own, or the built-in mode a custom one is based on. */
function modeColor(base: number) {
	if (base === SILENT) return GH_COLORS.eco;
	if (base === TURBO) return GH_COLORS.turbo;
	return GH_COLORS.standard;
}

const SECTION = 18 + 5 + 46;
const GAP = 8;

/** Panel height inside the open notch, below its 36 px header. */
export function ghelperPanelHeight(state: GHelperState | null, message: boolean) {
	const padding = 10 + 10;
	const top = 26 + GAP;
	if (!state || !state.installed || !state.running) return padding + top + 30 + (message ? 26 : 0);
	const gpu = state.gpu_mode >= 0 ? SECTION + GAP : 0;
	const battery = 18 + 5 + 20;
	return padding + top + SECTION + GAP + gpu + SECTION + GAP + battery + (message ? 26 : 0);
}

function sensorText(label: string, temp: number | null, fan: number | null) {
	const parts: string[] = [];
	if (temp != null) parts.push(`${label}: ${temp}°C`);
	if (fan != null) parts.push(`Fan: ${fan}RPM`);
	return parts.join("  ");
}

function ModeButton({
	label,
	icon: Icon,
	color,
	active,
	disabled,
	onClick
}: {
	label: string;
	/** Mode and GPU buttons have one; the screen's are text, as in G-Helper. */
	icon?: typeof Bike;
	color: string;
	active: boolean;
	disabled?: boolean;
	onClick: () => void;
}) {
	return (
		<button
			type="button"
			className={`gh-button${active ? " active" : ""}`}
			style={{ ["--gh-color" as string]: color }}
			aria-pressed={active}
			disabled={disabled}
			onClick={(e) => {
				e.stopPropagation();
				onClick();
			}}
		>
			{Icon && <Icon size={16} strokeWidth={1.8} />}
			<span>{label}</span>
		</button>
	);
}

export function GHelperPanel({
	onHeightChange,
	onOpenSettings
}: {
	onHeightChange: (height: number) => void;
	onOpenSettings: () => void;
}) {
	const [state, setState] = useState<GHelperState | null>(null);
	const [pending, setPending] = useState<string | null>(null);
	const [message, setMessage] = useState<string | null>(null);
	const [confirm, setConfirm] = useState<string | null>(null);
	const [customOpen, setCustomOpen] = useState(false);
	const [charge, setCharge] = useState<number | null>(null);
	const chargeTimer = useRef<number | null>(null);

	const refresh = useCallback(async () => {
		try {
			setState(await invoke<GHelperState>("ghelper_state", { sensors: true }));
		} catch {
			/* keeps the last one */
		}
	}, []);

	useEffect(() => {
		void refresh();
		const timer = window.setInterval(() => void refresh(), 2000);
		const changed = listen("ghelper-changed", () => void refresh());
		return () => {
			window.clearInterval(timer);
			void changed.then((f) => f());
		};
	}, [refresh]);

	const shown = message ?? (confirm ? "confirm" : null);
	useEffect(() => {
		onHeightChange(ghelperPanelHeight(state, !!shown));
	}, [state?.installed, state?.running, state?.gpu_mode, shown]);

	const run = async (action: string) => {
		setPending(action);
		setMessage(null);
		try {
			await invoke("ghelper_action", { action });
		} catch (e) {
			setMessage(String(e));
		} finally {
			setPending(null);
			void refresh();
		}
	};

	const showMessage = (text: string) => {
		setMessage(text);
		window.setTimeout(() => setMessage((m) => (m === text ? null : m)), 6000);
	};

	const top = (
		<div className="gh-top">
			<img
				className="gh-mark"
				src={TRAY_ICONS[state?.tray_icon ?? "standard"] ?? TRAY_ICONS.standard}
				alt=""
				draggable={false}
			/>
			<span className="gh-title">G-Helper</span>
			{(pending || state?.applying) && <Loader2 className="gh-spin" size={14} />}
			<button
				type="button"
				className="gh-icon"
				title="Open G-Helper"
				onClick={(e) => {
					e.stopPropagation();
					void run("open");
				}}
			>
				<ExternalLink size={13} />
			</button>
			<button
				type="button"
				className="gh-icon"
				title="G-Helper settings in Bloom (key bindings and shortcuts)"
				onClick={(e) => {
					e.stopPropagation();
					onOpenSettings();
				}}
			>
				<Settings2 size={13} />
			</button>
		</div>
	);

	if (!state) return <div className="gh-panel">{top}</div>;

	if (!state.installed || !state.running) {
		return (
			<div className="gh-panel" onClick={(e) => e.stopPropagation()}>
				{top}
				<div className="gh-empty">
					{state.installed ? "G-Helper isn't running" : "G-Helper wasn't found on this PC"}
					{state.installed && (
						<button
							type="button"
							className="gh-text-button"
							onClick={() =>
								void invoke("ghelper_start")
									.then(() => window.setTimeout(() => void refresh(), 2500))
									.catch((e) => showMessage(String(e)))
							}
						>
							Start G-Helper
						</button>
					)}
				</div>
				{message && <div className="gh-message">{message}</div>}
			</div>
		);
	}

	const s = state.sensors;
	const busy = !!pending || state.applying;
	// What was just asked for shows at once; G-Helper's own word follows.
	const modeNow = pending?.startsWith("mode:") ? Number(pending.slice(5)) : state.mode;
	const custom = state.modes.filter((m) => m.id > 2);
	const current = state.modes.find((m) => m.id === modeNow);
	const customActive = modeNow > 2;
	const gpuNow = pending?.startsWith("gpu:") ? pending.slice(4) : null;
	const gpuAuto = gpuNow ? gpuNow === "optimized" : state.gpu_auto;
	const gpuMode =
		gpuNow === "eco" ? ECO : gpuNow === "standard" ? STANDARD : gpuNow === "ultimate" ? ULTIMATE : state.gpu_mode;
	const gpuLabel =
		gpuMode === ECO ? "iGPU only" : gpuMode === ULTIMATE ? "dGPU exclusive" : "iGPU + dGPU";
	const chargeShown = charge ?? state.charge_limit;
	const screenOn = state.refresh > 0;

	const setGpu = (target: string) => {
		const leaving = state.gpu_mode === ULTIMATE && target !== "ultimate";
		if (target === "ultimate" && state.gpu_mode !== ULTIMATE) setConfirm("ultimate");
		else if (leaving) setConfirm(target);
		else void run(`gpu:${target}`);
	};

	return (
		<div className="gh-panel" onClick={(e) => e.stopPropagation()}>
			{top}

			<section className="gh-section">
				<div className="gh-head">
					<span className="gh-head-title">
						<Gauge size={13} />
						Mode: {current?.name ?? "—"}
					</span>
					<span className="gh-head-info">
						{s.cpu_text ?? sensorText("CPU", s.cpu_temp, s.cpu_fan)}
					</span>
				</div>
				<div className="gh-row">
					<ModeButton
						label="Silent"
						icon={Bike}
						color={GH_COLORS.eco}
						active={modeNow === SILENT}
						disabled={busy}
						onClick={() => void run(`mode:${SILENT}`)}
					/>
					<ModeButton
						label="Balanced"
						icon={Car}
						color={GH_COLORS.standard}
						active={modeNow === BALANCED}
						disabled={busy}
						onClick={() => void run(`mode:${BALANCED}`)}
					/>
					<ModeButton
						label="Turbo"
						icon={Rocket}
						color={GH_COLORS.turbo}
						active={modeNow === TURBO}
						disabled={busy}
						onClick={() => void run(`mode:${TURBO}`)}
					/>
					<div className="gh-custom">
						<ModeButton
							label={customActive && current ? current.name : "Fans + Power"}
							icon={Fan}
							color={customActive && current ? modeColor(current.base) : GH_COLORS.custom}
							active={customActive}
							disabled={busy}
							onClick={() => {
								if (custom.length === 0) void run("open");
								else setCustomOpen((open) => !open);
							}}
						/>
						{customOpen && (
							<div className="gh-menu" role="menu">
								{custom.map((m) => (
									<button
										key={m.id}
										type="button"
										role="menuitemradio"
										aria-checked={m.id === modeNow}
										className={m.id === modeNow ? "checked" : ""}
										style={{ ["--gh-color" as string]: modeColor(m.base) }}
										onClick={(e) => {
											e.stopPropagation();
											setCustomOpen(false);
											void run(`mode:${m.id}`);
										}}
									>
										{m.name}
									</button>
								))}
								<button
									type="button"
									role="menuitem"
									onClick={(e) => {
										e.stopPropagation();
										setCustomOpen(false);
										void run("open");
									}}
								>
									Fans + Power in G-Helper…
								</button>
							</div>
						)}
					</div>
				</div>
			</section>

			{state.gpu_mode >= 0 && (
				<section className="gh-section">
					<div className="gh-head">
						<span className="gh-head-title">
							<Gpu size={13} />
							GPU Mode: {gpuLabel}
						</span>
						<span className="gh-head-info">
							{/* G-Helper's own line; while it has none (its GPU reading
							    sleeps with the GPU), the hardware's own GPU sensor, which is
							    where G-Helper itself falls back to. */}
							{s.gpu_text ?? sensorText("GPU", s.gpu_temp, s.gpu_fan)}
						</span>
					</div>
					<div className="gh-row">
						<ModeButton
							label="Eco"
							icon={Leaf}
							color={GH_COLORS.eco}
							active={gpuMode === ECO && !gpuAuto}
							disabled={busy}
							onClick={() => setGpu("eco")}
						/>
						<ModeButton
							label="Standard"
							icon={Flower2}
							color={GH_COLORS.standard}
							active={gpuMode === STANDARD && !gpuAuto}
							disabled={busy}
							onClick={() => setGpu("standard")}
						/>
						<ModeButton
							label="Ultimate"
							icon={Gamepad2}
							color={GH_COLORS.turbo}
							active={gpuMode === ULTIMATE}
							disabled={busy || !state.has_ultimate}
							onClick={() => setGpu("ultimate")}
						/>
						<ModeButton
							label="Optimized"
							icon={Sparkles}
							color={gpuMode === ECO ? GH_COLORS.eco : GH_COLORS.standard}
							active={gpuAuto && gpuMode !== ULTIMATE}
							disabled={busy}
							onClick={() => setGpu("optimized")}
						/>
					</div>
				</section>
			)}

			<section className="gh-section">
				<div className="gh-head">
					<span className="gh-head-title">
						<Laptop size={13} />
						Screen: {screenOn ? `${state.refresh}Hz` : "Off"}
					</span>
					<span className="gh-head-info">
						{s.mid_fan != null ? `Mid Fan: ${s.mid_fan}RPM` : ""}
					</span>
				</div>
				<div className="gh-row">
					<ModeButton
						label="Auto"
						color={GH_COLORS.gray}
						active={state.screen_auto}
						disabled={busy || !screenOn}
						onClick={() => void run("screen:auto")}
					/>
					<ModeButton
						label={`${state.refresh_min}Hz`}
						color={GH_COLORS.gray}
						active={!state.screen_auto && state.refresh === state.refresh_min}
						disabled={busy || !screenOn}
						onClick={() => void run("screen:min")}
					/>
					<ModeButton
						label={`${state.refresh_max > 0 ? state.refresh_max : "Max "}Hz`}
						color={GH_COLORS.gray}
						active={!state.screen_auto && state.refresh > state.refresh_min}
						disabled={busy || !screenOn}
						onClick={() => void run("screen:max")}
					/>
					{state.miniled != null && (
						<ModeButton
							label={state.miniled === 2 ? "Multi Zone Strong" : state.miniled === 1 ? "Multi Zone" : "One Zone"}
								color={state.miniled === 2 ? GH_COLORS.turbo : GH_COLORS.standard}
							active={state.miniled > 0}
							disabled={busy}
							onClick={() => void run("miniled")}
						/>
					)}
				</div>
			</section>

			<section className="gh-section gh-battery">
				<div className="gh-head">
					<span className="gh-head-title">
						<BatteryCharging size={13} />
						Battery Charge Limit: {chargeShown}%
					</span>
				</div>
				<input
					type="range"
					className="gh-slider"
					min={40}
					max={100}
					step={5}
					value={chargeShown}
					disabled={busy}
					style={{ ["--gh-fill" as string]: `${((chargeShown - 40) / 60) * 100}%` }}
					onChange={(e) => {
						const value = Number(e.target.value);
						setCharge(value);
						// Applied once the slider rests: G-Helper restarts for it.
						if (chargeTimer.current) window.clearTimeout(chargeTimer.current);
						chargeTimer.current = window.setTimeout(() => {
							void run(`charge:${value}`).then(() => setCharge(null));
						}, 900);
					}}
				/>
			</section>

			{confirm && (
				<div className="gh-message gh-confirm">
					<span>
						{confirm === "ultimate"
							? "Ultimate needs a restart. Restart the PC now?"
							: "Leaving Ultimate needs a restart. Restart the PC now?"}
					</span>
					<button type="button" className="gh-text-button" onClick={() => setConfirm(null)}>
						Cancel
					</button>
					<button
						type="button"
						className="gh-text-button danger"
						onClick={() => {
							const target = confirm;
							setConfirm(null);
							void run(`gpu:${target}`);
						}}
					>
						Restart
					</button>
				</div>
			)}
			{!confirm && message && <div className="gh-message">{message}</div>}
		</div>
	);
}
