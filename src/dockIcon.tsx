import { memo, useEffect, useState } from "react";

/**
 * Icons drawn at the same size. Icons come with different amounts of empty
 * space around them (a 48 px icon centred in a 256 px image, Tray Bin's 16 px
 * art with a margin, shortcut icons with padding), so each one is cropped to
 * its visible pixels first.
 *
 * Cropped alone, a full square (RuneLite's tile, a Windows logo plate) then
 * looks much bigger than a round or thin icon of the same width. So, as
 * launchers on phones do, each icon is also scaled by how much of its box it
 * actually covers: every icon ends up covering about the same area, which is
 * what reads as "the same size". A circle fills the square, a full square is
 * drawn at about 89 % of it, and shapes in between in proportion. The result
 * is cached per image.
 */

const SIZE = 128;
/** The share of the square a circle covers; every icon is matched to it. */
const TARGET_COVERAGE = Math.PI / 4;
const done = new Map<string, string>();
const working = new Map<string, Promise<string>>();

function cropToContent(src: string, pixelArt: boolean): Promise<string> {
	const key = `${pixelArt ? "p" : "s"}:${src}`;
	const cached = done.get(key);
	if (cached) return Promise.resolve(cached);
	const running = working.get(key);
	if (running) return running;

	const job = (async () => {
		const image = new Image();
		image.src = src;
		await image.decode();
		// Vector images are drawn large so cropping never upscales a tiny raster.
		const vector = src.startsWith("data:image/svg") || /\.svg($|\?)/i.test(src);
		const factor = vector
			? 512 / Math.max(image.naturalWidth || 512, image.naturalHeight || 512)
			: 1;
		const width = Math.round((image.naturalWidth || 512) * factor);
		const height = Math.round((image.naturalHeight || 512) * factor);
		if (!width || !height) return src;

		const source = document.createElement("canvas");
		source.width = width;
		source.height = height;
		const sctx = source.getContext("2d", { willReadFrequently: true });
		if (!sctx) return src;
		sctx.drawImage(image, 0, 0, width, height);
		const { data } = sctx.getImageData(0, 0, width, height);

		let minX = width;
		let minY = height;
		let maxX = -1;
		let maxY = -1;
		for (let y = 0; y < height; y++) {
			for (let x = 0; x < width; x++) {
				if (data[(y * width + x) * 4 + 3] > 16) {
					if (x < minX) minX = x;
					if (x > maxX) maxX = x;
					if (y < minY) minY = y;
					if (y > maxY) maxY = y;
				}
			}
		}
		if (maxX < 0) return src;

		const boxW = maxX - minX + 1;
		const boxH = maxY - minY + 1;
		// The shape's area, measured per row from its first to its last
		// visible pixel, so a ring or a glyph with holes counts as solid.
		let area = 0;
		for (let y = minY; y <= maxY; y++) {
			let left = -1;
			let right = -1;
			for (let x = minX; x <= maxX; x++) {
				if (data[(y * width + x) * 4 + 3] > 16) {
					if (left < 0) left = x;
					right = x;
				}
			}
			if (left >= 0) area += right - left + 1;
		}
		const fit = SIZE / Math.max(boxW, boxH);
		const coverage = (area * fit * fit) / (SIZE * SIZE);
		// Never larger than fitting the square, and never tiny for odd shapes.
		const balance = coverage > TARGET_COVERAGE ? Math.sqrt(TARGET_COVERAGE / coverage) : 1;
		const scale = fit * Math.max(balance, 0.8);
		const drawW = boxW * scale;
		const drawH = boxH * scale;

		const out = document.createElement("canvas");
		out.width = SIZE;
		out.height = SIZE;
		const octx = out.getContext("2d");
		if (!octx) return src;
		octx.imageSmoothingEnabled = !pixelArt;
		octx.imageSmoothingQuality = "high";
		octx.drawImage(
			source,
			minX,
			minY,
			boxW,
			boxH,
			(SIZE - drawW) / 2,
			(SIZE - drawH) / 2,
			drawW,
			drawH
		);
		return out.toDataURL("image/png");
	})()
		.catch(() => src)
		.then((result) => {
			done.set(key, result);
			working.delete(key);
			return result;
		});
	working.set(key, job);
	return job;
}

interface DockIconImageProps {
	src: string;
	alt: string;
	className?: string;
	style?: React.CSSProperties;
	/** Small pixel art (Tray Bin's icons): scale without smoothing. */
	pixelArt?: boolean;
}

export const DockIconImage = memo(function DockIconImage({
	src,
	alt,
	className,
	style,
	pixelArt = false
}: DockIconImageProps) {
	const key = `${pixelArt ? "p" : "s"}:${src}`;
	const [shown, setShown] = useState<string | null>(() => done.get(key) ?? null);

	useEffect(() => {
		let alive = true;
		const cached = done.get(key);
		if (cached) {
			setShown(cached);
			return;
		}
		setShown(null);
		void cropToContent(src, pixelArt).then((result) => {
			if (alive) setShown(result);
		});
		return () => {
			alive = false;
		};
	}, [key, src, pixelArt]);

	return (
		<img
			src={shown ?? src}
			alt={alt}
			className={className}
			// Hidden for the moment it takes to crop, so it never jumps in size.
			style={shown ? style : { ...style, opacity: 0 }}
			draggable={false}
		/>
	);
});
