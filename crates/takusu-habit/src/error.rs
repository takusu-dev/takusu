use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid time of day: {hour}:{minute:02}")]
    InvalidTimeOfDay { hour: u8, minute: u8 },
    #[error("invalid recurrence rule: {0}")]
    InvalidRule(String),
    #[error("ambiguous or nonexistent time in timezone")]
    AmbiguousTime,
    #[error("date arithmetic overflow")]
    DateOverflow,
}