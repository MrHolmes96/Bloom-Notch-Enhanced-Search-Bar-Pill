import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
	Volume2,
	Sun,
	SunMedium,
	ArrowLeftToLine,
	ArrowRightToLine,
	Laptop
} from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredString } from "./useStoredSetting";

interface OverlaysTabProps {
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
	// G-Helper handles an ASUS laptop's own brightness keys with its own step.
	const [ghelper, setGhelper] = useState<{ step: number; running: boolean } | null>(null);
	const [ghelperNote, setGhelperNote] = useState<string | null>(null);
	useEffect(() => {
		void invoke<{ step: number; running: boolean } | null>("ghelper_brightness")
			.then(setGhelper)
			.catch(() => setGhelper(null));
	}, []);
	const wantedGhelperStep = keyStep === "0" ? 10 : Number(keyStep);
	const applyToGhelper = async () => {
		setGhelperNote("Updating G-Helper…");
		try {
			await invoke("set_ghelper_brightness_step", { step: wantedGhelperStep });
			setGhelper((g) => (g ? { ...g, step: wantedGhelperStep } : g));
			setGhelperNote(null);
		} catch (e) {
			setGhelperNote(String(e));
		}
	};
	return (
		<>
			<div className="setting-group-label">Overlays</div>
			<div className="setting-group">
				<SettingRow icon={Volume2} label="Volume HUD" desc="Bloom volume overlay">
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
					>
						<label className="toggle-switch">
							<input type="checkbox" checked={volumeEdgeEnabled} onChange={toggleVolumeEdge} />
							<span className="slider"></span>
						</label>
					</SettingRow>
				)}

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
							: `Each press of a brightness key changes it by ${keyStep}%`
					}
					divider={brightnessOverlayEnabled || ghelper !== null}
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

				{ghelper && (
					<SettingRow
						icon={Laptop}
						label="Laptop Keys (G-Helper)"
						desc={
							ghelperNote ??
							(ghelper.step === wantedGhelperStep
								? `G-Helper steps ${ghelper.step}% too, so the laptop's keys change it directly`
								: `G-Helper steps ${ghelper.step}%. Click to make it ${wantedGhelperStep}% (G-Helper restarts once)`)
						}
						action={ghelper.step !== wantedGhelperStep}
						divider={brightnessOverlayEnabled}
						onClick={ghelper.step !== wantedGhelperStep ? () => void applyToGhelper() : undefined}
					/>
				)}

				{brightnessOverlayEnabled && (
					<SettingRow
						icon={ArrowRightToLine}
						label="Show on Edge Hover"
						desc="Slide in from right edge"
						divider={false}
					>
						<label className="toggle-switch">
							<input
								type="checkbox"
								checked={brightnessEdgeEnabled}
								onChange={toggleBrightnessEdge}
							/>
							<span className="slider"></span>
						</label>
					</SettingRow>
				)}
			</div>
		</>
	);
}
