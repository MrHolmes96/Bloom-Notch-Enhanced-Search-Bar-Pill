/**
 * The ISO 8601 week number of a date (weeks start on Monday; week 1 is the
 * week with the year's first Thursday), as used in Denmark and most of
 * Europe.
 */
export function isoWeek(date: Date): number {
	const day = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()));
	// The Thursday of this week decides which year the week belongs to.
	const weekday = day.getUTCDay() || 7;
	day.setUTCDate(day.getUTCDate() + 4 - weekday);
	const yearStart = Date.UTC(day.getUTCFullYear(), 0, 1);
	return Math.ceil(((day.getTime() - yearStart) / 86_400_000 + 1) / 7);
}
