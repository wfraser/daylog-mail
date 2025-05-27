use anyhow::{anyhow, Context};
use chrono::NaiveDate;
use crate::db::UserRaw;
use crate::time::{DaylogTime, SleepTime};
use std::collections::BTreeMap;
use std::convert::TryFrom;

#[derive(Debug, Clone)]
pub struct User {
    //pub id: i64,
    pub username: String,
    pub email: String,
    pub timezone: chrono_tz::Tz,
    pub email_time_local: DaylogTime,
}

impl TryFrom<UserRaw> for User {
    type Error = anyhow::Error;
    fn try_from(raw: UserRaw) -> Result<Self, Self::Error> {
        Ok(User {
            //id: raw.id.ok_or_else(|| anyhow!("missing ID for user {:?}",raw.username))?,
            timezone: raw.timezone.as_str().parse::<chrono_tz::Tz>()
                .map_err(|e| {
                    // can't use with_context because of type bounds
                    anyhow!("failed to parse timezone for user {:?}: {}", raw.username, e)
                })?,
            email_time_local: DaylogTime::parse(&raw.email_time_local)
                .with_context(|| format!("failed to parse time for user {:?}", raw.username))?,
            email: raw.email,
            username: raw.username,
        })
    }
}

pub struct Users {
    vec: Vec<User>,
}

impl Users {
    pub fn new(users: Vec<User>) -> Self {
        Self {
            vec: users,
        }
    }

    /// Given a date and time, return the set of users who should be emailed next, and the time to
    /// sleep to until then. This needs a date because users' times are specified in local timezone,
    /// and local times depend what day it is, because daylight savings time exists.
    pub fn next_from_time(&self, date: NaiveDate, time: DaylogTime) -> Option<(SleepTime, Vec<User>)> {
        // Simple brute-force method: recalculate everyone's local time on every call.
        // This can probably be improved, because nobody's time can change more than once per day,
        // but this is fast enough for now.

        info!("getting users from DaylogTime {time} on {date}");
        let mut by_time = BTreeMap::<SleepTime, Vec<User>>::new();
        let now = date.and_time(time.as_naivetime()).and_utc();

        for user in &self.vec {
            let sleep_time = user.email_time_local.apply_timezone(now, &user.timezone);
            by_time.entry(sleep_time).or_default().push(user.to_owned());
        }

        by_time.into_iter().next()
    }
}
