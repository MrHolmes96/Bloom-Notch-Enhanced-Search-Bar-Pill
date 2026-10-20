import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
	Bell,
	CornerUpRight,
	PanelRight,
	MoveDiagonal,
	Send,
	ShieldAlert,
	Timer
} from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredString } from "./useStoredSetting";

/** Settings > Notifications' sub-sections. */
export type NotificationsSection = "popups" | "center";

/** Settings > Notifications: where notifications appear and how they look. */
export function NotificationsTab({ section }: { section: NotificationsSection }) {
	const [style, setStyle] = useStoredString("bloom-notify-style", "windows");
	const [position, setPosition] = useStoredString("bloom-notify-position", "default");
	const [radius, setRadius] = useStoredString("bloom-notify-radius", "16");
	const [duration, setDuration] = useStoredString("bloom-notify-duration", "6");
	const [chosenCenter, setCenter] = useStoredString("bloom-notification-center", "");
	const [status, setStatus] = useState<string>("off");
	const bloom = style === "bloom";
	// Until one is chosen, the bell opens Bloom's center while notifications
	// are Bloom cards, so every card is there too.
	const center = chosenCenter || (bloom ? "bloom" : "windows");

	useEffect(() => {
		void invoke<string>("notifications_status")
			.then(setStatus)
			.catch(() => {});
		const unlisten = listen<string>("notifications-status", (e) => setStatus(e.payload));
		return () => {
			void unlisten.then((fn) => fn());
		};
	}, []);

	const problem =
		bloom && status === "denied"
			? "Windows doesn't let Bloom read notifications. Turn on notification access for apps"
			: bloom && status === "unavailable"
				? "Windows won't share notifications with Bloom, so Windows' own pop-ups are kept"
				: null;

	if (section === "center") {
		return (
			<>
				<div className="setting-group-label">Notification Center</div>
				<div className="setting-group">
					<SettingRow
						icon={PanelRight}
						label="Notification Center"
						desc={
							center === "bloom"
								? "The bell opens Bloom's own list, in Bloom's look, sized to what's in it, grouped by app"
								: "The bell opens Windows' notification center"
						}
						divider={false}
					>
						<select
							className="settings-select"
							value={center}
							onChange={(e) => setCenter(e.target.value)}
						>
							<option value="windows">Windows</option>
							<option value="bloom">Bloom</option>
						</select>
					</SettingRow>
				</div>
			</>
		);
	}

	return (
		<>
			<div className="setting-group-label">Pop-ups</div>
			<div className="setting-group">
				<SettingRow
					icon={Bell}
					label="Style"
					desc={
						bloom
							? "Bloom cards in Bloom's colors and opacity, with the notification's own buttons and replies"
							: "Windows' own pop-ups"
					}
				>
					<select
						className="settings-select"
						value={style}
						onChange={(e) => setStyle(e.target.value)}
					>
						<option value="windows">Windows</option>
						<option value="bloom">Bloom</option>
					</select>
				</SettingRow>
				<SettingRow
					icon={CornerUpRight}
					label="Position"
					desc="Which corner of the screen notifications come from"
					divider={bloom}
				>
					<select
						className="settings-select"
						value={position}
						onChange={(e) => setPosition(e.target.value)}
					>
						<option value="default">Windows default</option>
						<option value="top-right">Top right</option>
						<option value="top-left">Top left</option>
						<option value="bottom-right">Bottom right</option>
						<option value="bottom-left">Bottom left</option>
					</select>
				</SettingRow>
				{bloom && (
					<>
						<SettingRow icon={MoveDiagonal} label="Corner Rounding" desc={`${radius} px`}>
							<input
								type="range"
								min="0"
								max="28"
								step="1"
								value={radius}
								className="settings-slider"
								onChange={(e) => setRadius(e.target.value)}
							/>
						</SettingRow>
						<SettingRow
							icon={Timer}
							label="Show For"
							desc="How long a card stays (hover to keep it)"
						>
							<select
								className="settings-select"
								value={duration}
								onChange={(e) => setDuration(e.target.value)}
							>
								<option value="4">4 seconds</option>
								<option value="6">6 seconds</option>
								<option value="8">8 seconds</option>
								<option value="10">10 seconds</option>
								<option value="15">15 seconds</option>
							</select>
						</SettingRow>
						{problem && (
							<SettingRow
								icon={ShieldAlert}
								label="Notification Access"
								desc={problem}
								action
								onClick={() => void invoke("notifications_open_privacy")}
							/>
						)}
						<SettingRow
							icon={Send}
							label="Try It"
							desc="Show a sample card"
							action
							divider={false}
							onClick={() => void invoke("notifications_test")}
						/>
					</>
				)}
			</div>
		</>
	);
}
