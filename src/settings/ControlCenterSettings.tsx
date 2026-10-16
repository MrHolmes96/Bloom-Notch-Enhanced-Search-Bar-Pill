import {
	Bell,
	Captions,
	Cast,
	ChevronDown,
	ChevronUp,
	ChevronsUp,
	Contrast,
	Keyboard,
	Leaf,
	MonitorUp,
	Moon,
	PersonStanding,
	Plane,
	Plus,
	RadioTower,
	RotateCcw,
	RotateCcwSquare,
	RotateCw,
	Settings,
	Share2,
	Shield,
	SunMoon,
	X
} from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredString } from "./useStoredSetting";
import {
	CC_BUTTONS,
	CC_DEFAULT,
	CC_KEY,
	CC_PER_ROW,
	ccButtonInfo,
	parseCCButtons,
	serializeCCButtons,
	type CCButtonId
} from "../controlCenter";

const ICONS: Record<CCButtonId, typeof Bell> = {
	dnd: Moon,
	"energy-saver": Leaf,
	tray: ChevronsUp,
	notifications: Bell,
	"bloom-settings": Settings,
	"restart-bloom": RotateCw,
	"night-light": SunMoon,
	airplane: Plane,
	hotspot: RadioTower,
	"rotation-lock": RotateCcwSquare,
	"live-captions": Captions,
	project: MonitorUp,
	cast: Cast,
	"keyboard-layout": Keyboard,
	accessibility: PersonStanding,
	"nearby-sharing": Share2,
	vpn: Shield,
	"color-filters": Contrast
};

/** Settings > Notch > Control Center: which round buttons show, and their order. */
export function ControlCenterSettings() {
	const [raw, setRaw] = useStoredString(CC_KEY, serializeCCButtons(CC_DEFAULT));
	const shown = parseCCButtons(raw);
	const hidden = CC_BUTTONS.filter((b) => !shown.includes(b.id));
	const save = (next: CCButtonId[]) => setRaw(serializeCCButtons(next));

	const move = (index: number, by: -1 | 1) => {
		const next = [...shown];
		const target = index + by;
		if (target < 0 || target >= next.length) return;
		[next[index], next[target]] = [next[target], next[index]];
		save(next);
	};

	const rows = Math.ceil(shown.length / CC_PER_ROW);
	const isDefault = serializeCCButtons(shown) === serializeCCButtons(CC_DEFAULT);

	return (
		<>
			<div className="setting-group-label">Control Center</div>
			<div className="setting-group">
				{shown.map((id, index) => {
					const info = ccButtonInfo(id);
					return (
						<SettingRow
							key={id}
							icon={ICONS[id]}
							label={info.label}
							desc={info.windows ? `${info.desc} (Windows)` : info.desc}
						>
							<div className="cc-edit-buttons">
								<button
									type="button"
									className="extra-folder-remove"
									title="Move up"
									disabled={index === 0}
									onClick={() => move(index, -1)}
								>
									<ChevronUp size={13} />
								</button>
								<button
									type="button"
									className="extra-folder-remove"
									title="Move down"
									disabled={index === shown.length - 1}
									onClick={() => move(index, 1)}
								>
									<ChevronDown size={13} />
								</button>
								<button
									type="button"
									className="extra-folder-remove"
									title="Remove"
									onClick={() => save(shown.filter((s) => s !== id))}
								>
									<X size={13} />
								</button>
							</div>
						</SettingRow>
					);
				})}
				<div
					className="setting-item"
					style={{ flexDirection: "column", alignItems: "flex-start", gap: "10px" }}
				>
					<div style={{ display: "flex", alignItems: "center", width: "100%" }}>
						<div className="setting-icon-bg">
							<Plus size={14} strokeWidth={1.5} />
						</div>
						<div className="setting-info">
							<span className="setting-label">Add a Button</span>
							<span className="setting-desc">
								{shown.length === 0
									? "The round buttons are hidden. Add one to show the row again"
									: `${shown.length} button${shown.length === 1 ? "" : "s"} in ${rows} row${rows === 1 ? "" : "s"} of up to ${CC_PER_ROW}`}
							</span>
						</div>
					</div>
					{hidden.length > 0 && (
						<div className="cc-add-chips">
							{hidden.map((b) => {
								const Icon = ICONS[b.id];
								return (
									<button
										key={b.id}
										type="button"
										className="folder-chip"
										title={b.desc}
										onClick={() => save([...shown, b.id])}
									>
										<Icon size={12} />
										{b.label}
									</button>
								);
							})}
						</div>
					)}
				</div>
				{!isDefault && (
					<>
						<div className="setting-divider" />
						<SettingRow
							icon={RotateCcw}
							label="Reset Buttons"
							desc="Back to Bloom's six buttons"
							action
							divider={false}
							onClick={() => save([...CC_DEFAULT])}
						/>
					</>
				)}
			</div>
		</>
	);
}
