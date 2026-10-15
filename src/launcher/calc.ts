/**
 * The search bar's calculator: + - * / % ^, factorial (!), brackets,
 * implicit multiplication (2pi, 3(4+1)), hex (0xff) and binary (0b101)
 * numbers, constants and functions such as sqrt, min, max, log and sin.
 * Pure logic, no Tauri imports, so it can be tested on its own.
 */

export type DecimalMode = "dot" | "comma";

const CONSTANTS: Record<string, number> = {
	pi: Math.PI,
	e: Math.E,
	tau: Math.PI * 2,
	phi: (1 + Math.sqrt(5)) / 2
};

function factorial(n: number): number {
	if (n < 0 || !Number.isInteger(n)) return NaN;
	if (n > 170) return Infinity;
	let r = 1;
	for (let i = 2; i <= n; i++) r *= i;
	return r;
}

const FUNCTIONS: Record<string, (...args: number[]) => number> = {
	sqrt: Math.sqrt,
	cbrt: Math.cbrt,
	abs: Math.abs,
	sign: Math.sign,
	floor: Math.floor,
	ceil: Math.ceil,
	round: (x, digits = 0) => {
		const f = Math.pow(10, digits);
		return Math.round(x * f) / f;
	},
	trunc: Math.trunc,
	exp: Math.exp,
	ln: Math.log,
	log: (x, base) => (base === undefined ? Math.log10(x) : Math.log(x) / Math.log(base)),
	log10: Math.log10,
	log2: Math.log2,
	sin: Math.sin,
	cos: Math.cos,
	tan: Math.tan,
	asin: Math.asin,
	acos: Math.acos,
	atan: Math.atan,
	atan2: Math.atan2,
	sinh: Math.sinh,
	cosh: Math.cosh,
	tanh: Math.tanh,
	min: (...a) => (a.length ? Math.min(...a) : NaN),
	max: (...a) => (a.length ? Math.max(...a) : NaN),
	sum: (...a) => a.reduce((s, x) => s + x, 0),
	avg: (...a) => (a.length ? a.reduce((s, x) => s + x, 0) / a.length : NaN),
	pow: Math.pow,
	hypot: Math.hypot,
	mod: (a, b) => a % b,
	fact: factorial,
	factorial,
	rad: (deg) => (deg * Math.PI) / 180,
	deg: (rad) => (rad * 180) / Math.PI
};

type Token =
	| { kind: "num"; value: number }
	| { kind: "name"; value: string }
	| { kind: "op"; value: string };

/** Turns locale-style numbers into plain ones and picks the argument separator. */
function normalize(input: string, mode: DecimalMode): string {
	let src = input.replace(/×/g, "*").replace(/÷/g, "/").replace(/−/g, "-").replace(/\*\*/g, "^");
	const hasFunction = /[a-z]\s*\(/i.test(src);
	if (mode === "comma") {
		if (hasFunction) {
			// min(1,5) — commas separate arguments, so decimals use a dot.
			src = src.replace(/;/g, ",");
		} else {
			// 1.234,5 — dots group thousands, the comma is the decimal mark.
			src = src.replace(/(\d)\.(?=\d{3}(\D|$))/g, "$1").replace(/,/g, ".");
		}
	} else {
		src = src.replace(/;/g, ",");
		if (!hasFunction) src = src.replace(/(\d),(?=\d{3}(\D|$))/g, "$1");
	}
	return src;
}

function tokenize(src: string): Token[] | null {
	const tokens: Token[] = [];
	let i = 0;
	while (i < src.length) {
		const c = src[i];
		if (c === " " || c === "\t") {
			i++;
			continue;
		}
		const hex = /^0x[0-9a-f]+/i.exec(src.slice(i));
		if (hex) {
			tokens.push({ kind: "num", value: parseInt(hex[0].slice(2), 16) });
			i += hex[0].length;
			continue;
		}
		const bin = /^0b[01]+/i.exec(src.slice(i));
		if (bin) {
			tokens.push({ kind: "num", value: parseInt(bin[0].slice(2), 2) });
			i += bin[0].length;
			continue;
		}
		const num = /^(\d+\.?\d*|\.\d+)(e[+-]?\d+)?/i.exec(src.slice(i));
		if (num) {
			const value = Number(num[0]);
			if (!Number.isFinite(value)) return null;
			tokens.push({ kind: "num", value });
			i += num[0].length;
			continue;
		}
		const name = /^[a-z][a-z0-9]*/i.exec(src.slice(i));
		if (name) {
			tokens.push({ kind: "name", value: name[0].toLowerCase() });
			i += name[0].length;
			continue;
		}
		if ("+-*/%^()!,".includes(c)) {
			tokens.push({ kind: "op", value: c });
			i++;
			continue;
		}
		return null;
	}
	return tokens;
}

/**
 * Evaluates an expression, or returns null when the text isn't a
 * calculation (a plain number or an app name falls through to other results).
 */
export function evaluate(input: string, mode: DecimalMode = "dot"): number | null {
	// Without a digit it's a name ("e", "pi", "steam"), not a calculation.
	if (!/\d/.test(input)) return null;
	const tokens = tokenize(normalize(input.trim(), mode));
	if (!tokens || tokens.length === 0) return null;
	// Something has to be computed: an operator, a function or a constant.
	const meaningful = tokens.some(
		(t, idx) =>
			(t.kind === "op" && t.value !== "(" && t.value !== ")" && !(idx === 0 && t.value === "-")) ||
			t.kind === "name"
	);
	if (!meaningful) return null;

	let pos = 0;
	const peek = () => tokens[pos];
	const isOp = (v: string) => {
		const t = peek();
		return t?.kind === "op" && t.value === v;
	};
	// A value can directly follow another one: 2pi, 3(4), (1)(2).
	const startsValue = () => {
		const t = peek();
		return !!t && (t.kind === "num" || t.kind === "name" || (t.kind === "op" && t.value === "("));
	};

	const parseExpr = (): number => {
		let value = parseTerm();
		while (isOp("+") || isOp("-")) {
			const op = tokens[pos++].value;
			const rhs = parseTerm();
			value = op === "+" ? value + rhs : value - rhs;
		}
		return value;
	};
	const parseTerm = (): number => {
		let value = parseUnary();
		for (;;) {
			if (isOp("*") || isOp("/") || isOp("%")) {
				const op = tokens[pos++].value;
				const rhs = parseUnary();
				value = op === "*" ? value * rhs : op === "/" ? value / rhs : value % rhs;
			} else if (startsValue()) {
				value *= parseUnary();
			} else {
				return value;
			}
		}
	};
	const parseUnary = (): number => {
		if (isOp("-")) {
			pos++;
			return -parseUnary();
		}
		if (isOp("+")) {
			pos++;
			return parseUnary();
		}
		return parsePower();
	};
	const parsePower = (): number => {
		const base = parsePostfix();
		if (isOp("^")) {
			pos++;
			return Math.pow(base, parseUnary());
		}
		return base;
	};
	const parsePostfix = (): number => {
		let value = parsePrimary();
		while (isOp("!")) {
			pos++;
			value = factorial(value);
		}
		return value;
	};
	const parsePrimary = (): number => {
		const t = tokens[pos++];
		if (!t) throw new Error("end");
		if (t.kind === "num") return t.value;
		if (t.kind === "name") {
			const fn = FUNCTIONS[t.value];
			if (fn && isOp("(")) {
				pos++;
				const args: number[] = [];
				if (!isOp(")")) {
					args.push(parseExpr());
					while (isOp(",")) {
						pos++;
						args.push(parseExpr());
					}
				}
				if (!isOp(")")) throw new Error("bracket");
				pos++;
				return fn(...args);
			}
			if (t.value in CONSTANTS) return CONSTANTS[t.value];
			throw new Error("name");
		}
		if (t.value === "(") {
			const v = parseExpr();
			if (!isOp(")")) throw new Error("bracket");
			pos++;
			return v;
		}
		throw new Error("token");
	};

	try {
		const result = parseExpr();
		if (pos !== tokens.length || !Number.isFinite(result)) return null;
		return result;
	} catch {
		return null;
	}
}

/** Formats a result for display and copying. */
export function formatResult(
	value: number,
	opts: { mode: DecimalMode; maxDecimals: number; thousands: boolean }
): string {
	const locale = opts.mode === "comma" ? "de-DE" : "en-US";
	const rounded = Number(value.toFixed(Math.min(Math.max(opts.maxDecimals, 0), 20)));
	if (Math.abs(rounded) >= 1e21) return rounded.toExponential(6);
	return new Intl.NumberFormat(locale, {
		maximumFractionDigits: Math.min(Math.max(opts.maxDecimals, 0), 20),
		useGrouping: opts.thousands
	}).format(rounded === 0 ? 0 : rounded);
}

/** The system's decimal mark, for the "Use system setting" option. */
export function systemDecimalMode(): DecimalMode {
	return (1.5).toLocaleString().includes(",") ? "comma" : "dot";
}
