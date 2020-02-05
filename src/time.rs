use chrono::prelude::*;
use chrono::Duration;
use failure::bail;
use std::cmp::Ordering;

/// Daylog operates in UTC, with minute resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaylogTime {
    hour: u8,
    minute: u8,
}

impl DaylogTime {
    pub fn now() -> (Date<Utc>, Self) {
        let now = Utc::now();
        (now.date(), Self::from(now.time()))
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

    pub fn as_naivetime(self) -> NaiveTime {
        NaiveTime::from_hms(
            u32::from(self.hour),
            u32::from(self.minute),
            0,
        )
    }

    fn from_naivetime(t: NaiveTime) -> Self {
        Self {
            hour: t.hour() as u8,
            minute: t.minute() as u8,
        }
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
            bail!("hour is out of range");
        }

        let minute: u8 = parts.next().ok_or_else(|| failure::err_msg("couldn't find minute"))?
            .parse().map_err(|e| failure::err_msg(format!("bad minute: {}", e)))?;
        if minute > 59 {
            bail!("minute is out of range");
        }

        Ok(Self { hour, minute })
    }

    pub fn apply_timezone<Tz: TimeZone>(self, utc_now: DateTime<Utc>, tz: &Tz) -> SleepTime {
        let adj = |date: NaiveDate| -> DateTime<Tz> {
            let local = date.and_time(self.as_naivetime());
            match tz.from_local_datetime(&local) {
                chrono::LocalResult::None => {
                    // caller asked for something like 2:01am during a DST transition
                    // pick 1 hour later and assume it will work...
                    let later = local + Duration::hours(1);
                    tz.from_local_datetime(&later).unwrap()
                }
                other => other.latest().unwrap(),
            }
        };

        let local_today = adj(utc_now.naive_utc().date());

        if local_today.naive_utc().time() >= utc_now.time() {
            SleepTime::Today(Self::from_naivetime(
                local_today.naive_utc().time()))
        } else {
            let tomorrow = utc_now.naive_utc().date().succ();
            let local_tomorrow = adj(tomorrow);
            SleepTime::Tomorrow(Self::from_naivetime(
                local_tomorrow.naive_utc().time()))
        }
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

impl Ord for DaylogTime {
    fn cmp(&self, other: &DaylogTime) -> Ordering {
        match self.hour.cmp(&other.hour) {
            Ordering::Greater => Ordering::Greater,
            Ordering::Less => Ordering::Less,
            Ordering::Equal => self.minute.cmp(&other.minute),
        }
    }
}

impl PartialOrd for DaylogTime {
    fn partial_cmp(&self, other: &DaylogTime) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Represents a time to be waited until.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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

impl std::fmt::Display for SleepTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SleepTime::Today(time) => write!(f, "{} today", time),
            SleepTime::Tomorrow(time) => write!(f, "{} tomorrow", time),
        }
    }
}

impl Ord for SleepTime {
    fn cmp(&self, other: &SleepTime) -> Ordering {
        match (self, other) {
            (SleepTime::Today(a), SleepTime::Today(b))
                | (SleepTime::Tomorrow(a), SleepTime::Tomorrow(b)) =>
            {
                a.cmp(b)
            }
            (SleepTime::Tomorrow(_), SleepTime::Today(_)) => Ordering::Greater,
            (SleepTime::Today(_), SleepTime::Tomorrow(_)) => Ordering::Less,
        }
    }
}

impl PartialOrd for SleepTime {
    fn partial_cmp(&self, other: &SleepTime) -> Option<Ordering> {
        Some(self.cmp(other))
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

    #[test]
    fn test_timezone() {
        // 2020-03-08 is the day that PST becomes PDT at 2:00 AM local time (10:00 AM UTC).
        let tz = chrono_tz::America::Los_Angeles;
        let email_time = DaylogTime::new(18, 0); // 6:00 PM in America/Los_Angeles

        // Let's say it's 2020-03-07, before the time change.
        // Assert that 6PM email will be sent at 2AM UTC.
        let mut utc_now = Utc.ymd(2020, 3, 7).and_hms(0, 0, 0);
        let x1 = email_time.apply_timezone(utc_now, &tz);
        assert_eq!(x1, SleepTime::Today(DaylogTime { hour: 2, minute: 0 }));

        // Let's advance just past that time.
        // Assert that the email gets sent tomorrow, since it's too late today, and that the time
        // changes because then it'll be after the time change to PDT.
        utc_now = Utc.ymd(2020, 3, 7).and_hms(2, 1, 0);
        let x2 = email_time.apply_timezone(utc_now, &tz);
        assert_eq!(x2, SleepTime::Tomorrow(DaylogTime { hour: 1, minute: 0 }));

        // Now it's 10:01 AM UTC, right after PST turns to PDT.
        // Assert that the PDT time tomorrow is still picked.
        utc_now = Utc.ymd(2020, 3, 7).and_hms(10, 1, 0);
        let x3 = email_time.apply_timezone(utc_now, &tz);
        assert_eq!(x3, SleepTime::Tomorrow(DaylogTime { hour: 1, minute: 0 }));

        // Now it's the next day. Assert that it's sent today, at the right time.
        utc_now = Utc.ymd(2020, 3, 8).and_hms(0, 0, 0);
        let x4 = email_time.apply_timezone(utc_now, &tz);
        assert_eq!(x4, SleepTime::Today(DaylogTime { hour: 1, minute: 0 }));
    }
}
