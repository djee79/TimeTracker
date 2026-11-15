//! Date logic behind days off: Easter, Québec statutory holidays and
//! range expansion. Pure functions, no database.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

/// Easter Sunday (Gregorian), anonymous Gregorian computus
/// (Meeus/Jones/Butcher).
pub fn easter(year: i32) -> NaiveDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(year, month as u32, day as u32).unwrap_or_default()
}

/// Which Easter holiday the employer gives (Québec lets them choose).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasterDay {
    GoodFriday,
    EasterMonday,
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap_or_default()
}

/// The `n`-th (1-based) `weekday` of a month.
fn nth_weekday(year: i32, month: u32, weekday: Weekday, n: u32) -> NaiveDate {
    NaiveDate::from_weekday_of_month_opt(year, month, weekday, n as u8).unwrap_or_default()
}

/// Québec statutory holidays for `year`, in date order, with their names.
pub fn quebec_holidays(year: i32, easter_day: EasterDay) -> Vec<(NaiveDate, &'static str)> {
    let e = easter(year);
    let easter_holiday = match easter_day {
        EasterDay::GoodFriday => (e - Duration::days(2), "Good Friday"),
        EasterDay::EasterMonday => (e + Duration::days(1), "Easter Monday"),
    };
    // National Patriots' Day: the Monday before May 25
    let mut patriots = ymd(year, 5, 24);
    while patriots.weekday() != Weekday::Mon {
        patriots -= Duration::days(1);
    }
    vec![
        (ymd(year, 1, 1), "New Year’s Day"),
        easter_holiday,
        (patriots, "National Patriots’ Day"),
        (ymd(year, 6, 24), "Fête nationale"),
        (ymd(year, 7, 1), "Canada Day"),
        (nth_weekday(year, 9, Weekday::Mon, 1), "Labour Day"),
        (nth_weekday(year, 10, Weekday::Mon, 2), "Thanksgiving"),
        (ymd(year, 12, 25), "Christmas Day"),
    ]
}

/// Every weekday (Mon–Fri) in [from, to] inclusive — what a "Range…" or
/// an office closure turns into, one day-off row each. Reversed bounds are
/// swapped.
pub fn weekdays_in(from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let (from, to) = if from <= to { (from, to) } else { (to, from) };
    from.iter_days()
        .take_while(|d| *d <= to)
        .filter(|d| d.weekday().num_days_from_monday() < 5)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn easter_known_years() {
        for (y, date) in [
            (1818, "1818-03-22"), // earliest possible
            (1943, "1943-04-25"), // latest possible
            (2000, "2000-04-23"),
            (2019, "2019-04-21"),
            (2024, "2024-03-31"),
            (2025, "2025-04-20"),
            (2026, "2026-04-05"),
            (2027, "2027-03-28"),
            (2038, "2038-04-25"),
        ] {
            assert_eq!(easter(y), d(date), "{y}");
        }
    }

    #[test]
    fn quebec_2026() {
        let h = quebec_holidays(2026, EasterDay::GoodFriday);
        let dates: Vec<NaiveDate> = h.iter().map(|(d, _)| *d).collect();
        assert_eq!(
            dates,
            [
                d("2026-01-01"),
                d("2026-04-03"), // Good Friday
                d("2026-05-18"), // May 25 is a Monday → the Monday before
                d("2026-06-24"),
                d("2026-07-01"),
                d("2026-09-07"),
                d("2026-10-12"),
                d("2026-12-25"),
            ]
        );
        let h = quebec_holidays(2026, EasterDay::EasterMonday);
        assert_eq!(h[1], (d("2026-04-06"), "Easter Monday"));
    }

    #[test]
    fn patriots_day_and_mondays_other_years() {
        // 2025: May 25 is a Sunday → May 19
        assert_eq!(quebec_holidays(2025, EasterDay::GoodFriday)[2].0, d("2025-05-19"));
        // 2027: May 25 is a Tuesday → May 24
        let h = quebec_holidays(2027, EasterDay::GoodFriday);
        assert_eq!(h[2].0, d("2027-05-24"));
        assert_eq!(h[5].0, d("2027-09-06")); // Labour Day
        assert_eq!(h[6].0, d("2027-10-11")); // Thanksgiving
        assert_eq!(h[1].0, d("2027-03-26")); // Good Friday
    }

    #[test]
    fn range_expands_to_weekdays() {
        // Fri 2026-11-27 .. Tue 2026-12-01: skips the weekend
        assert_eq!(
            weekdays_in(d("2026-11-27"), d("2026-12-01")),
            vec![d("2026-11-27"), d("2026-11-30"), d("2026-12-01")]
        );
        // reversed bounds, year boundary
        assert_eq!(
            weekdays_in(d("2027-01-01"), d("2026-12-31")),
            vec![d("2026-12-31"), d("2027-01-01")]
        );
        assert!(weekdays_in(d("2026-11-28"), d("2026-11-29")).is_empty());
        assert_eq!(weekdays_in(d("2026-11-27"), d("2026-11-27")).len(), 1);
    }
}
