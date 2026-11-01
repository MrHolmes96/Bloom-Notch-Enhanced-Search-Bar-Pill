/**
 * The desktop menu's icons, drawn after Windows 11's own (16 px, thin
 * outlines, the accent parts in Windows' light blue), so the menu Bloom
 * shows beside the dock looks like the one Windows shows elsewhere.
 */

const WHITE = "#ffffff";
const BLUE = "#60cdff";

const svg = (children: React.ReactNode) => (
	<svg
		viewBox="0 0 16 16"
		width="16"
		height="16"
		fill="none"
		strokeWidth="1"
		strokeLinecap="round"
		strokeLinejoin="round"
		aria-hidden="true"
	>
		{children}
	</svg>
);

export const ViewIcon = () =>
	svg(
		<g stroke={WHITE}>
			<rect x="2" y="2" width="5" height="5" rx="1.5" />
			<rect x="9" y="2" width="5" height="5" rx="1.5" />
			<rect x="2" y="9" width="5" height="5" rx="1.5" />
			<rect x="9" y="9" width="5" height="5" rx="1.5" />
		</g>
	);

export const SortIcon = () =>
	svg(
		<>
			<path d="M5 13.5V2.5M2 5.5l3-3 3 3" stroke={WHITE} />
			<path d="M11 2.5v11M8 10.5l3 3 3-3" stroke={BLUE} />
		</>
	);

export const RefreshIcon = () =>
	svg(<path d="M13.5 8A5.5 5.5 0 1 1 11.9 4.1M12.5 1.5v3h-3" stroke={WHITE} />);

export const NewIcon = () =>
	svg(
		<g stroke={WHITE}>
			<circle cx="8" cy="8" r="6.5" />
			<path d="M8 5v6M5 8h6" />
		</g>
	);

export const DisplayIcon = () =>
	svg(
		<>
			<path
				d="M7.5 11.5H13a1.5 1.5 0 0 0 1.5-1.5V4A1.5 1.5 0 0 0 13 2.5H3A1.5 1.5 0 0 0 1.5 4v3.5M8.5 14h3"
				stroke={WHITE}
			/>
			<g stroke={BLUE}>
				<circle cx="4" cy="12" r="1.4" />
				<path d="M4 9.1v1.1M4 13.8v1.1M1.1 12h1.1M5.8 12h1.1M1.95 9.95l.75.75M5.3 13.3l.75.75M1.95 14.05l.75-.75M5.3 10.7l.75-.75" />
			</g>
		</>
	);

export const PersonaliseIcon = () =>
	svg(
		<g stroke={BLUE}>
			<path d="M13.4 1.6a1 1 0 0 1 1.4 1.4L9.6 8.6 7.9 6.9Z" />
			<path d="M7.9 6.9 9.6 8.6c.3 1.6-.6 3.1-2.1 3.6-1.6.6-3.6.4-5.6 1.9.9-2 .8-3.9 1.4-5.5.5-1.4 2.2-2.4 4.6-1.7Z" />
		</g>
	);

export const TerminalIcon = () =>
	svg(
		<>
			<rect x="1.5" y="2.5" width="13" height="11" rx="2" fill="#2b2b2b" stroke="#9a9a9a" />
			<path d="M4.5 6.5 6.5 8l-2 1.5M8 10h3" stroke={WHITE} />
		</>
	);

export const MoreIcon = () =>
	svg(
		<>
			<path d="M7 3.5H4A1.5 1.5 0 0 0 2.5 5v7A1.5 1.5 0 0 0 4 13.5h7a1.5 1.5 0 0 0 1.5-1.5V9" stroke={WHITE} />
			<path d="M9.5 2.5h4v4M13.5 2.5 8 8" stroke={BLUE} />
		</>
	);

export const FolderIcon = () =>
	svg(
		<path
			d="M1.5 4.5A1.5 1.5 0 0 1 3 3h3l1.5 1.5H13A1.5 1.5 0 0 1 14.5 6v5.5A1.5 1.5 0 0 1 13 13H3a1.5 1.5 0 0 1-1.5-1.5Z"
			fill="#ffd75e"
			stroke="#e0b330"
		/>
	);

export const FileIcon = () =>
	svg(<path d="M4 1.5h5L12.5 5v9.5h-8.5zM9 1.5V5h3.5" stroke={WHITE} />);
