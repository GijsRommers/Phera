//! Proleptic Gregorian arithmetic: `gmdate('Y-m-d', -62167219200)` is `0000-01-01`.
//! Wide intermediates keep every signed 64-bit Unix timestamp representable.

#[derive(Clone, Copy, Debug)]
pub struct Civil {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
}

pub fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

pub fn month_days(year: i64, month: i64) -> i64 {
    match month {
        2 => 28 + i64::from(leap(year)),
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Count complete March-based 400-year eras, including before year zero.
pub fn days(year: i64, month: i64, day: i64) -> i128 {
    let year = i128::from(year) + (i128::from(month) - 1).div_euclid(12);
    let month = (i128::from(month) - 1).rem_euclid(12) + 1;
    let year = year - i128::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let march_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * march_month + 2) / 5 + i128::from(day) - 1;
    era * 146097 + year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year - 719468
}

impl Civil {
    pub fn from_timestamp(timestamp: i64) -> Self {
        let day_number = i128::from(timestamp).div_euclid(86400);
        let shifted = day_number + 719468;
        let era = shifted.div_euclid(146097);
        let day_of_era = shifted - era * 146097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
        let year = year_of_era + era * 400;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let march_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * march_month + 2) / 5 + 1;
        let month = march_month + if march_month < 10 { 3 } else { -9 };
        let clock = timestamp.rem_euclid(86400);
        Self {
            year: (year + i128::from(month <= 2)) as i64,
            month: month as i64,
            day: day as i64,
            hour: clock / 3600,
            minute: clock / 60 % 60,
            second: clock % 60,
        }
    }

    pub fn timestamp(self) -> i128 {
        days(self.year, self.month, self.day) * 86400
            + i128::from(self.hour) * 3600
            + i128::from(self.minute) * 60
            + i128::from(self.second)
    }

    pub fn weekday(self) -> i64 {
        (days(self.year, self.month, self.day) + 4).rem_euclid(7) as i64
    }

    pub fn yearday(self) -> i64 {
        (days(self.year, self.month, self.day) - days(self.year, 1, 1)) as i64
    }

    pub fn iso_week(self) -> (i64, i64) {
        let day = days(self.year, self.month, self.day);
        let thursday = day + 3 - i128::from((self.weekday() + 6) % 7);
        let mut year = self.year;
        if thursday < days(year, 1, 1) {
            year -= 1;
        } else if thursday >= days(year + 1, 1, 1) {
            year += 1;
        }
        (year, ((thursday - days(year, 1, 1)) / 7 + 1) as i64)
    }
}
