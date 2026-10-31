import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Loader2 } from "lucide-react";
import "./KeyboardLayoutMenu.css";

/**
 * The control center's keyboard button: the keyboard layouts set up in
 * Windows (as Win+Space lists them), the one in use ticked. Picking one
 * switches the app being typed in to it; the menu closes only once Windows
 * confirms the switch, otherwise it says why it didn't happen.
 */

interface Layout {
	id: string;
	language: string;
	keyboard: string;
	short: string;
	active: boolean;
}

export function KeyboardLayoutMenu({
	top,
	maxHeight,
	onHeight,
	onClose
}: {
	top: number;
	maxHeight: number;
	/** Its height, so the control center makes room for it. */
	onHeight: (height: number) => void;
	onClose: () => void;
}) {
	const [layouts, setLayouts] = useState<Layout[] | null>(null);
	const [switching, setSwitching] = useState<string | null>(null);
	const [problem, setProblem] = useState<string | null>(null);
	const menuRef = useRef<HTMLDivElement | null>(null);

	useLayoutEffect(() => {
		const el = menuRef.current;
		if (el) onHeight(Math.min(el.scrollHeight, maxHeight));
	}, [layouts, problem, maxHeight, onHeight]);

	useEffect(() => {
		invoke<Layout[]>("keyboard_layouts")
			.then(setLayouts)
			.catch((e) => {
				setLayouts([]);
				setProblem(String(e));
			});
	}, []);

	useEffect(() => {
		const onKey = (e: KeyboardEvent) => {
			if (e.key === "Escape") onClose();
		};
		window.addEventListener("keydown", onKey);
		return () => window.removeEventListener("keydown", onKey);
	}, [onClose]);

	const pick = async (layout: Layout) => {
		if (switching) return;
		if (layout.active) {
			onClose();
			return;
		}
		setSwitching(layout.id);
		setProblem(null);
		try {
			await invoke("switch_keyboard_layout", { id: layout.id });
			setLayouts((prev) => prev?.map((l) => ({ ...l, active: l.id === layout.id })) ?? prev);
			window.setTimeout(onClose, 250);
		} catch (e) {
			setProblem(String(e));
		} finally {
			setSwitching(null);
		}
	};

	return (
		<>
			<div
				className="kb-menu-backdrop"
				onClick={(e) => {
					e.stopPropagation();
					onClose();
				}}
			/>
			<div
				ref={menuRef}
				className="kb-menu"
				style={{ top, maxHeight }}
				role="listbox"
				aria-label="Keyboard layouts"
				onClick={(e) => e.stopPropagation()}
			>
				{layouts === null && (
					<div className="kb-menu-note">
						<Loader2 className="kb-menu-spin" /> Reading your keyboards…
					</div>
				)}
				{layouts?.map((layout) => (
					<button
						key={layout.id}
						type="button"
						role="option"
						aria-selected={layout.active}
						className={`kb-menu-row${layout.active ? " is-on" : ""}`}
						disabled={!!switching}
						onClick={() => void pick(layout)}
					>
						<span className="kb-menu-short">{layout.short || "—"}</span>
						<span className="kb-menu-names">
							<span className="kb-menu-language">{layout.language}</span>
							{layout.keyboard && <span className="kb-menu-keyboard">{layout.keyboard}</span>}
						</span>
						{switching === layout.id ? (
							<Loader2 className="kb-menu-spin" />
						) : (
							layout.active && <Check className="kb-menu-check" />
						)}
					</button>
				))}
				{problem && <div className="kb-menu-note is-problem">{problem}</div>}
			</div>
		</>
	);
}
