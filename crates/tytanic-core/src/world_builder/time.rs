use chrono::{DateTime, Datelike, FixedOffset, Local, Utc};
use std::sync::Mutex;
use typst::foundations::Datetime;

/// Provides access to the system date, but not time.
pub struct SystemDateProvider {
    today: Mutex<Option<DateTime<Utc>>>,
}

impl SystemDateProvider {
    /// Reset the compilation state in preparation of a new compilation.
    pub fn reset(&self) {
        *self.today.lock().unwrap() = None;
    }
}

impl SystemDateProvider {
    /// The current system date.
    pub fn today(&self) -> DateTime<Utc> {
        *self.today.lock().unwrap().get_or_insert_with(Utc::now)
    }
}

impl SystemDateProvider {
    /// The current system date.
    pub fn today_with_offset(&self, offset: Option<i64>) -> Option<Datetime> {
        with_offset(self.today(), offset)
    }
}

/// Provides access to a fixed date, but not time.
pub struct FixedDateProvider {
    date: DateTime<Utc>,
}

impl FixedDateProvider {
    /// Create a new fixed date provider with the given date.
    pub fn new(date: DateTime<Utc>) -> Self {
        Self { date }
    }
}

impl FixedDateProvider {
    /// The fixed date.
    pub fn date(&self) -> DateTime<Utc> {
        self.date
    }
}

impl FixedDateProvider {
    /// The fixed date.
    pub fn date_with_offset(&self, offset: Option<i64>) -> Option<Datetime> {
        with_offset(self.date, offset)
    }
}

fn with_offset(today: DateTime<Utc>, offset: Option<i64>) -> Option<Datetime> {
    // The time with the specified UTC offset, or within the local time zone.
    let with_offset = match offset {
        Some(hours) => {
            let seconds = i32::try_from(hours).ok()?.checked_mul(3600)?;
            today.with_timezone(&FixedOffset::east_opt(seconds)?)
        }
        None => today.with_timezone(&Local).fixed_offset(),
    };

    Datetime::from_ymd(
        with_offset.year(),
        with_offset.month().try_into().ok()?,
        with_offset.day().try_into().ok()?,
    )
}
