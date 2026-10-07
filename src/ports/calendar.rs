//! UTC dates from seconds since 1970, shared by the adapters for logs, HTTP and signed requests.
//! No clock is read here: callers pass the time.

/// A moment in UTC, broken into its calendar parts.
pub struct Utc {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u64,
    pub minute: u64,
    pub second: u64,
    /// 0 is Sunday.
    pub weekday: u64,
}

impl Utc {
    pub fn from_unix(secs: u64) -> Utc {
        let days = i64::try_from(secs / 86_400).unwrap_or(0);
        let rem = secs % 86_400;
        // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        Utc {
            year: yoe + era * 400 + i64::from(month <= 2),
            month: u32::try_from(month).unwrap_or(1),
            day: u32::try_from(day).unwrap_or(1),
            hour: rem / 3_600,
            minute: (rem % 3_600) / 60,
            second: rem % 60,
            // 1970-01-01 was a Thursday.
            weekday: (secs / 86_400 + 4) % 7,
        }
    }

    /// 2026-10-07T11:31:05Z
    pub fn iso(&self) -> String {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", self.year, self.month, self.day, self.hour, self.minute, self.second)
    }

    /// 20261007T113105Z, the form AWS signatures use.
    pub fn compact(&self) -> String {
        format!("{:04}{:02}{:02}T{:02}{:02}{:02}Z", self.year, self.month, self.day, self.hour, self.minute, self.second)
    }

    /// Wed, 07 Oct 2026 11:31:05 GMT, the form HTTP's Date header uses.
    pub fn http(&self) -> String {
        const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let wd = DAYS[usize::try_from(self.weekday).unwrap_or(0) % 7];
        let mon = MONTHS[usize::try_from(self.month).unwrap_or(1).clamp(1, 12) - 1];
        format!("{wd}, {:02} {mon} {:04} {:02}:{:02}:{:02} GMT", self.day, self.year, self.hour, self.minute, self.second)
    }
}

#[cfg(test)]
mod tests {
    use super::Utc;

    #[test]
    fn dates() {
        assert_eq!(Utc::from_unix(0).iso(), "1970-01-01T00:00:00Z");
        assert_eq!(Utc::from_unix(951_782_400).iso(), "2000-02-29T00:00:00Z");
        assert_eq!(Utc::from_unix(1_440_938_160).compact(), "20150830T123600Z");
        // RFC 9110's example date.
        assert_eq!(Utc::from_unix(784_111_777).http(), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(Utc::from_unix(1_791_372_665).iso(), "2026-10-07T11:31:05Z");
    }
}
