use jiff::civil::Date;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    pub fn from_jiff(wd: jiff::civil::Weekday) -> Self {
        match wd {
            jiff::civil::Weekday::Monday => Weekday::Mon,
            jiff::civil::Weekday::Tuesday => Weekday::Tue,
            jiff::civil::Weekday::Wednesday => Weekday::Wed,
            jiff::civil::Weekday::Thursday => Weekday::Thu,
            jiff::civil::Weekday::Friday => Weekday::Fri,
            jiff::civil::Weekday::Saturday => Weekday::Sat,
            jiff::civil::Weekday::Sunday => Weekday::Sun,
        }
    }

    pub fn to_jiff(self) -> jiff::civil::Weekday {
        match self {
            Weekday::Mon => jiff::civil::Weekday::Monday,
            Weekday::Tue => jiff::civil::Weekday::Tuesday,
            Weekday::Wed => jiff::civil::Weekday::Wednesday,
            Weekday::Thu => jiff::civil::Weekday::Thursday,
            Weekday::Fri => jiff::civil::Weekday::Friday,
            Weekday::Sat => jiff::civil::Weekday::Saturday,
            Weekday::Sun => jiff::civil::Weekday::Sunday,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NWeekday {
    pub n: Option<i8>,
    pub weekday: Weekday,
}

impl NWeekday {
    pub fn every(weekday: Weekday) -> Self {
        Self { n: None, weekday }
    }

    pub fn nth(n: i8, weekday: Weekday) -> Self {
        Self {
            n: Some(n),
            weekday,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecurrenceRule {
    pub freq: Frequency,
    pub interval: u32,
    pub by_day: Vec<NWeekday>,
    pub by_month: Vec<i8>,
    pub by_month_day: Vec<i8>,
    pub count: Option<u32>,
    pub exdates: Vec<Date>,
}

impl RecurrenceRule {
    pub fn daily() -> Self {
        Self {
            freq: Frequency::Daily,
            interval: 1,
            by_day: vec![],
            by_month: vec![],
            by_month_day: vec![],
            count: None,
            exdates: vec![],
        }
    }

    pub fn weekly() -> Self {
        Self {
            freq: Frequency::Weekly,
            interval: 1,
            by_day: vec![],
            by_month: vec![],
            by_month_day: vec![],
            count: None,
            exdates: vec![],
        }
    }

    pub fn monthly() -> Self {
        Self {
            freq: Frequency::Monthly,
            interval: 1,
            by_day: vec![],
            by_month: vec![],
            by_month_day: vec![],
            count: None,
            exdates: vec![],
        }
    }

    pub fn yearly() -> Self {
        Self {
            freq: Frequency::Yearly,
            interval: 1,
            by_day: vec![],
            by_month: vec![],
            by_month_day: vec![],
            count: None,
            exdates: vec![],
        }
    }

    pub fn interval(mut self, interval: u32) -> Self {
        self.interval = interval;
        self
    }

    pub fn by_day(mut self, days: Vec<NWeekday>) -> Self {
        self.by_day = days;
        self
    }

    pub fn by_month(mut self, months: Vec<i8>) -> Self {
        self.by_month = months;
        self
    }

    pub fn by_month_day(mut self, days: Vec<i8>) -> Self {
        self.by_month_day = days;
        self
    }

    pub fn count(mut self, count: u32) -> Self {
        self.count = Some(count);
        self
    }

    pub fn exdates(mut self, dates: Vec<Date>) -> Self {
        self.exdates = dates;
        self
    }
}
