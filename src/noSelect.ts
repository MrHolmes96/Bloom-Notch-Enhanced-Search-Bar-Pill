/**
 * Bloom's windows are controls, not pages: holding the mouse button and
 * dragging must never select their text. A selection (the notch clock turning
 * blue) also kept the button "busy", so the next clicks did nothing. Text
 * boxes still select and edit as usual. The CSS of each window does the same;
 * this also covers text a component re-enables by accident.
 */

function editable(target: EventTarget | null): boolean {
	const element = target instanceof Element ? target : (target as Node | null)?.parentElement;
	if (!element) return false;
	return !!element.closest("input, textarea, [contenteditable=''], [contenteditable='true']");
}

document.addEventListener(
	"selectstart",
	(event) => {
		if (!editable(event.target)) event.preventDefault();
	},
	{ capture: true }
);

// A stray selection (from a double click, say) is dropped as soon as the
// button goes down anywhere outside a text box.
document.addEventListener(
	"mousedown",
	(event) => {
		if (editable(event.target)) return;
		const selection = window.getSelection();
		if (selection && !selection.isCollapsed) selection.removeAllRanges();
	},
	{ capture: true }
);

export {};
