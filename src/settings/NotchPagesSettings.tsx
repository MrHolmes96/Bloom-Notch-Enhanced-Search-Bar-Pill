import { useEffect, useState } from "react";
import { Reorder, useDragControls } from "framer-motion";
import { Calendar, Gauge, GripVertical, Inbox, LayoutGrid, ListTodo, PanelTop, Zap } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag, useStoredString } from "./useStoredSetting";
import {
	PAGE_LABELS,
	PAGE_ORDER_KEY,
	isNotchPageId,
	parsePageOrder,
	type NotchPageId
} from "../notchPages";

const PAGE_ICONS: Record<NotchPageId, typeof Calendar> = {
	calendar: Calendar,
	"command-center": LayoutGrid,
	shelf: Inbox,
	notes: ListTodo,
	blitzit: Zap,
	ghelper: Gauge
};

const PAGE_DESCRIPTIONS: Record<NotchPageId, string> = {
	calendar: "Calendar and the focus timer",
	"command-center": "Media, quick controls and tray; always on",
	shelf: "Dropped files and text, shown even when empty",
	notes: "Quick notes and to-dos",
	blitzit: "Blitzit's tasks (Integrations > Blitzit)",
	ghelper: "G-Helper's modes, GPU, screen and charge limit, on PCs with G-Helper"
};

/** "Where you left it" in the opening-page list. */
const REMEMBER = "remember";

function PageRow({
	id,
	on,
	onToggle,
	onDragEnd
}: {
	id: NotchPageId;
	on: boolean;
	onToggle: (() => void) | null;
	onDragEnd: () => void;
}) {
	const controls = useDragControls();
	const Icon = PAGE_ICONS[id];
	return (
		<Reorder.Item
			as="div"
			value={id}
			dragListener={false}
			dragControls={controls}
			onDragEnd={onDragEnd}
			className={`page-order-item${on ? "" : " is-off"}`}
		>
			<div className="setting-item">
				<button
					type="button"
					className="page-order-grip"
					title="Drag to change the order"
					aria-label={`Move ${PAGE_LABELS[id]}`}
					onPointerDown={(e) => controls.start(e)}
				>
					<GripVertical size={14} strokeWidth={1.6} />
				</button>
				<div className="setting-icon-bg">
					<Icon size={14} strokeWidth={1.5} />
				</div>
				<div className="setting-info">
					<span className="setting-label">{PAGE_LABELS[id]}</span>
					<span className="setting-desc">{PAGE_DESCRIPTIONS[id]}</span>
				</div>
				<label className={`toggle-switch${onToggle ? "" : " is-locked"}`}>
					<input type="checkbox" checked={on} disabled={!onToggle} onChange={() => onToggle?.()} />
					<span className="slider"></span>
				</label>
			</div>
		</Reorder.Item>
	);
}

/**
 * Settings > Notch > Pages: the order scrolling, swiping and the arrows go
 * through the notch's pages, which are on, and the page a click opens.
 */
export function NotchPagesSettings() {
	const [savedOrder, setSavedOrder] = useStoredString(PAGE_ORDER_KEY, "");
	const [order, setOrder] = useState<NotchPageId[]>(() => parsePageOrder(savedOrder));
	useEffect(() => setOrder(parsePageOrder(savedOrder)), [savedOrder]);

	const flags: Record<NotchPageId, [boolean, ((next: boolean) => void) | null]> = {
		calendar: useStoredFlag("bloom-calendar-enabled", true),
		"command-center": [true, null],
		shelf: useStoredFlag("bloom-notch-shelf-in-scroll", true),
		notes: useStoredFlag("bloom-notch-notes-in-scroll", true),
		blitzit: useStoredFlag("bloom-notch-blitzit", true),
		ghelper: useStoredFlag("bloom-ghelper-enabled", true)
	};
	const [remember, setRemember] = useStoredFlag(
		"bloom-notch-remember-panel",
		localStorage.getItem("bloom-notch-click-to-open") !== "false"
	);
	const [defaultPage, setDefaultPage] = useStoredString("bloom-notch-default-panel", "calendar");
	const opening = remember
		? REMEMBER
		: isNotchPageId(defaultPage) && flags[defaultPage][0]
			? defaultPage
			: "command-center";

	const chooseOpening = (value: string) => {
		if (value === REMEMBER) {
			setRemember(true);
			return;
		}
		setDefaultPage(value);
		setRemember(false);
	};

	return (
		<>
			<div className="setting-group-label">Order</div>
			<div className="setting-group">
				<Reorder.Group
					as="div"
					axis="y"
					values={order}
					onReorder={setOrder}
					className="page-order-list"
				>
					{order.map((id) => {
						const [on, set] = flags[id];
						return (
							<PageRow
								key={id}
								id={id}
								on={on}
								onToggle={set ? () => set(!on) : null}
								onDragEnd={() => setSavedOrder(JSON.stringify(order))}
							/>
						);
					})}
				</Reorder.Group>
			</div>
			<div className="setting-group-note">
				Drag the pages into the order scrolling and the arrows go through them. Pages that
				are off are skipped; what they hold is kept.
			</div>

			<div className="setting-group-label">Opening</div>
			<div className="setting-group">
				<SettingRow
					icon={PanelTop}
					label="Opens On"
					desc="The page a click on the notch opens"
					divider={false}
				>
					<select
						className="settings-select"
						value={opening}
						onChange={(e) => chooseOpening(e.target.value)}
					>
						<option value={REMEMBER}>Where you left it</option>
						{order
							.filter((id) => flags[id][0])
							.map((id) => (
								<option key={id} value={id}>
									{PAGE_LABELS[id]}
								</option>
							))}
					</select>
				</SettingRow>
			</div>
		</>
	);
}
