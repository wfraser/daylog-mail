use chrono::{NaiveTime, Timelike};

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

    pub fn duration_from(self, earlier_time: NaiveTime) -> chrono::Duration {
        self.as_naivetime().signed_duration_since(earlier_time)
    }

    pub fn duration_since_start_of_day(self) -> chrono::Duration {
        chrono::Duration::hours(i64::from(self.hour))
            + chrono::Duration::minutes(i64::from(self.minute))
    }

    pub fn format(self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
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
        f.write_str(&self.format())
    }
}

/*impl Deref for DaylogTime {
    type Target = NaiveTime;
    fn deref(&self) -> &NaiveTime {
        &NaiveTime::from_hms(u32::from(self.hour), u32::from(self.minute), 0)
    }
}*/

/// Represents a time to be waited until.
#[derive(Debug, Copy, Clone)]
pub enum SleepTime {
    Tomorrow(DaylogTime),
    Today(DaylogTime),
}

impl SleepTime {
    pub fn duration_from(self, earlier_time: NaiveTime) -> chrono::Duration {
        match self {
            SleepTime::Tomorrow(time) => {
                chrono::Duration::days(1)
                    - (chrono::Duration::hours(i64::from(earlier_time.hour()))
                        + chrono::Duration::minutes(i64::from(earlier_time.minute()))
                        + chrono::Duration::seconds(i64::from(earlier_time.second())))
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
    fn test_format() {
        assert_eq!("23:59", DaylogTime::new(23, 59).format());
        assert_eq!("00:00", DaylogTime::new(0, 0).format());
    }

    #[test]
    fn test_parse() {
        assert_eq!(DaylogTime::new(23, 59), DaylogTime::parse("23:59").unwrap());
        assert_eq!(DaylogTime::new(0, 0), DaylogTime::parse("00:00").unwrap());
        assert!(DaylogTime::parse("99:99").is_err());
    }
}

