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

    pub fn new(hour: u8, minute: u8) -> Self {
        assert!(hour < 24);
        assert!(minute < 60);
        Self { hour, minute }
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

    pub fn duration_until(self, other: NaiveTime) -> chrono::Duration {
        -(other - self.as_naivetime())
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
        let mut hour = time.hour() as u8;
        let mut minute = time.minute() as u8;
        if time.second() > 0 {
            minute += 1;
            if minute == 60 {
                minute = 0;
                hour += 1;
                if hour == 24 {
                    hour = 0;
                }
            }
        }
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

