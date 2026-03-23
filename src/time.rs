/// Convert days since Unix epoch to (year, month, day).
pub fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut month = 1u64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

pub fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_start() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn known_date() {
        // 2024-01-01 = 19723 days since epoch
        assert_eq!(days_to_ymd(19723), (2024, 1, 1));
    }

    #[test]
    fn leap_year_feb_29() {
        // 2024 is a leap year; day 59 (0-indexed) = Feb 29
        // Jan 1 2024 = day 19723, so Feb 29 = 19723 + 59 = 19782
        assert_eq!(days_to_ymd(19782), (2024, 2, 29));
    }

    #[test]
    fn non_leap_year() {
        assert!(!is_leap(2023));
        assert!(is_leap(2024));
        assert!(!is_leap(1900));
        assert!(is_leap(2000));
    }
}
