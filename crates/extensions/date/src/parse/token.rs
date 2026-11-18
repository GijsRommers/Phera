//! Scanner actions are independent of matching: `12:00 13:00` keeps the first time.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub len: usize,
    pub rank: usize,
    pub action: Action,
}

#[derive(Clone, Copy, Debug)]
pub struct DateToken {
    pub fields: [Option<i64>; 3],
    pub replace_year: bool,
    pub overflow: bool,
}
impl DateToken {
    pub fn new(year: Option<i64>, month: i64, day: Option<i64>) -> Self {
        Self {
            fields: [year, Some(month), day],
            replace_year: false,
            overflow: false,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TimeToken {
    pub fields: [i64; 4],
    pub zone: Option<usize>,
}
#[derive(Clone, Copy, Debug)]
pub enum Keyword {
    Now,
    Yesterday,
    Tomorrow,
    Midnight,
    Noon,
    Ago,
    First,
    Last,
}
#[derive(Clone, Copy, Debug)]
pub enum Unit {
    Field(usize, i64),
    Weekday(i64),
    Weekdays,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Special {
    #[default]
    None,
    Weekdays(i64),
    FirstWeekday,
    LastWeekday,
}
#[derive(Clone, Copy, Debug)]
pub enum Action {
    Keyword(Keyword),
    Date(DateToken),
    Time(TimeToken),
    Compound(DateToken, TimeToken, bool),
    Year(i64),
    Gnu(i64, i64, i64),
    Zone,
    Epoch,
    Week(i64, i64, i64),
    Relative {
        amount: i64,
        unit: Unit,
        behavior: WeekdayBehavior,
        reset: bool,
        week: bool,
        of: bool,
    },
    BareWeekday(i64),
    Clock(i64, i64),
}

/// Whether a weekday may include the reference day or belongs to a Monday-based week.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum WeekdayBehavior {
    #[default]
    After,
    OnOrAfter,
    ThisWeek,
}
