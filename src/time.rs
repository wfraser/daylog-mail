use chrono::{Duration, NaiveTime, Timelike};

/// Daylog operates in UTC, with minute resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaylogTime {
    hour: u8,
    minute: u8,
}

impl DaylogTime {
    pub fn now() -> Self {
        let time = chrono::Utc::now().time();
        Self::from(time)
    }

    #[cfg(test)]
    pub fn new(hour: u8, minute: u8) -> Self {
        Self { hour, minute }
    }

    pub fn zero() -> Self {
        Self {
            hour: 0,
            minute: 0,
        }
    }

    pub fn succ(self) -> Self {
        let mut minute = self.minute;
        let mut hour = self.hour;
        minute += 1;
        if minute == 60 {
            hour += 1;
            minute = 0;
            if hour == 24 {
                hour = 0;
            }
        }
        Self { hour, minute }
    }

    fn as_naivetime(self) -> NaiveTime {
        NaiveTime::from_hms(
            u32::from(self.hour),
            u32::from(self.minute),
            0,
        )
    }

    pub fn duration_from(self, earlier_time: NaiveTime) -> Duration {
        self.as_naivetime().signed_duration_since(earlier_time)
    }

    pub fn duration_since_start_of_day(self) -> Duration {
        Duration::hours(i64::from(self.hour))
            + Duration::minutes(i64::from(self.minute))
    }

    pub fn parse(s: &str) -> Result<Self, failure::Error> {
        let mut parts = s.splitn(2, ':');
        let hour: u8 = parts.next().ok_or_else(|| failure::err_msg("couldn't find hour"))?
            .parse().map_err(|e| failure::err_msg(format!("bad hour: {}", e)))?;
        if hour > 23 {
            return Err(failure::err_msg("hour is out of range"));
        }

        let minute: u8 = parts.next().ok_or_else(|| failure::err_msg("couldn't find minute"))?
            .parse().map_err(|e| failure::err_msg(format!("bad minute: {}", e)))?;
        if minute > 59 {
            return Err(failure::err_msg("minute is out of range"));
        }

        Ok(Self { hour, minute })
    }
}

impl From<NaiveTime> for DaylogTime {
    fn from(time: NaiveTime) -> Self {
        let hour = time.hour() as u8;
        let minute = time.minute() as u8;
        Self { hour, minute }
    }
}

impl std::fmt::Display for DaylogTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

/// Represents a time to be waited until.
#[derive(Debug, Copy, Clone)]
pub enum SleepTime {
    Tomorrow(DaylogTime),
    Today(DaylogTime),
}

impl SleepTime {
    pub fn duration_from(self, earlier_time: NaiveTime) -> Duration {
        match self {
            SleepTime::Tomorrow(time) => {
                Duration::days(1)
                    - (earlier_time - NaiveTime::from_hms(0, 0, 0))
                    + time.duration_since_start_of_day()
            }
            SleepTime::Today(time) => {
                time.duration_from(earlier_time)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_duration() {
        assert_eq!(
            Duration::hours(47) + Duration::minutes(59),
            SleepTime::Tomorrow(DaylogTime::new(23, 59))
                .duration_from(NaiveTime::from_hms(0,0,0)));
        assert_eq!(
            Duration::seconds(-56),
            SleepTime::Today(DaylogTime::new(12, 34))
                .duration_from(NaiveTime::from_hms(12, 34, 56)));
        assert_eq!(
            Duration::seconds(1),
            SleepTime::Tomorrow(DaylogTime::zero())
                .duration_from(NaiveTime::from_hms(23, 59, 59)));
    }

    #[test]
    fn test_format() {
        assert_eq!("23:59", DaylogTime::new(23, 59).to_string());
        assert_eq!("00:00", DaylogTime::new(0, 0).to_string());
    }

    #[test]
    fn test_parse() {
        assert_eq!(DaylogTime::new(23, 59), DaylogTime::parse("23:59").unwrap());
        assert_eq!(DaylogTime::new(0, 0), DaylogTime::parse("00:00").unwrap());
        assert!(DaylogTime::parse("99:99").is_err());
    }
}
