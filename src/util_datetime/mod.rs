pub mod data;

use crate::util_datetime::data::{AsData, DateTime};
use anyhow::{bail, Result};
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Add, AddAssign, BitAnd, BitOr, BitOrAssign, Bound, RangeBounds, Shl, Sub};
use time::{Duration, OffsetDateTime, PrimitiveDateTime};

#[derive(Clone)]
pub struct MonthDays(u32);
#[derive(Clone)]
pub struct WeekDays(u8);
#[derive(Clone)]
pub struct Hours(u32);
#[derive(Clone)]
pub struct Minuters(u64);
#[derive(Clone)]
pub struct Seconds(u64);
impl Operator for Hours {
    const MIN: Self::ValTy = 0;
    const MAX: Self::ValTy = 23;
    const ONE: Self::ValTy = 1;
    const ZERO: Self::ValTy = 0;
    const DEFAULT_MAX: Self::ValTy = u32::MAX >> 8;
    type ValTy = u32;
    fn _default() -> Self {
        Self(0)
    }
    fn _val(&self) -> Self::ValTy {
        self.0
    }
    fn _mut_val(&mut self, val: Self::ValTy) {
        self.0 = val
    }
}
impl Operator for Seconds {
    const MIN: Self::ValTy = 0;
    const MAX: Self::ValTy = 59;
    const ONE: Self::ValTy = 1;
    const ZERO: Self::ValTy = 0;
    const DEFAULT_MAX: Self::ValTy = u64::MAX >> 4;
    type ValTy = u64;
    fn _default() -> Self {
        Self(0)
    }
    fn _val(&self) -> Self::ValTy {
        self.0
    }
    fn _mut_val(&mut self, val: Self::ValTy) {
        self.0 = val
    }
}
impl Operator for Minuters {
    const MIN: Self::ValTy = 0;
    const MAX: Self::ValTy = 59;
    const ONE: Self::ValTy = 1;
    const ZERO: Self::ValTy = 0;
    const DEFAULT_MAX: Self::ValTy = u64::MAX >> 4;
    type ValTy = u64;
    fn _default() -> Self {
        Self(0)
    }
    fn _val(&self) -> Self::ValTy {
        self.0
    }
    fn _mut_val(&mut self, val: Self::ValTy) {
        self.0 = val
    }
}

impl Operator for MonthDays {
    const MIN: Self::ValTy = 1;
    const MAX: Self::ValTy = 31;
    const ONE: Self::ValTy = 1;
    const ZERO: Self::ValTy = 0;
    const DEFAULT_MAX: Self::ValTy = u32::MAX << 1;
    type ValTy = u32;
    fn _default() -> Self {
        Self(0)
    }

    fn _val(&self) -> Self::ValTy {
        self.0
    }
    fn _mut_val(&mut self, val: Self::ValTy) {
        self.0 = val
    }
}
impl Operator for WeekDays {
    const MIN: Self::ValTy = 1;
    const MAX: Self::ValTy = 7;
    const ONE: Self::ValTy = 1;
    const ZERO: Self::ValTy = 0;
    const DEFAULT_MAX: Self::ValTy = u8::MAX << 1;
    type ValTy = u8;

    fn _default() -> Self {
        Self(0)
    }

    fn _val(&self) -> Self::ValTy {
        self.0
    }
    fn _mut_val(&mut self, val: Self::ValTy) {
        self.0 = val
    }
}

pub trait Operator: Sized {
    const MIN: Self::ValTy;
    const MAX: Self::ValTy;
    const ONE: Self::ValTy;
    const ZERO: Self::ValTy;
    const DEFAULT_MAX: Self::ValTy;
    type ValTy: BitOr<Output = Self::ValTy>
        + Shl<Output = Self::ValTy>
        + Copy
        + BitOrAssign
        + Add<Output = Self::ValTy>
        + Sub<Output = Self::ValTy>
        + PartialOrd
        + AddAssign
        + BitAnd<Output = Self::ValTy>
        + Display;

    fn _default() -> Self;
    #[inline]
    fn default_value(val: impl AsData<Self::ValTy>) -> Self {
        let ins = Self::_default();
        ins.add(val)
    }
    #[inline]
    fn default_range<A: AsData<Self::ValTy>>(range: impl RangeBounds<A>) -> Result<Self> {
        let ins = Self::_default();
        ins.add_range(range)
    }
    #[inline]
    fn default_all() -> Self {
        let mut ins = Self::_default();
        ins._mut_val(Self::DEFAULT_MAX);
        ins
    }
    fn default_array(vals: &[impl AsData<Self::ValTy>]) -> Self {
        let ins = Self::_default();
        ins.add_array(vals)
    }
    fn add_array(mut self, vals: &[impl AsData<Self::ValTy>]) -> Self {
        let mut val = self._val();
        for i in vals {
            val |= Self::ONE << i.as_data();
        }
        self._mut_val(val);
        self
    }
    fn add(mut self, index: impl AsData<Self::ValTy>) -> Self {
        let index = index.as_data();
        self._mut_val(self._val() | (Self::ONE << index));
        self
    }
    fn add_range<A: AsData<Self::ValTy>>(mut self, range: impl RangeBounds<A>) -> Result<Self> {
        let mut first = match range.start_bound() {
            Bound::Unbounded => Self::MIN,
            Bound::Included(first) => first.as_data(),
            Bound::Excluded(first) => first.as_data() + Self::ONE,
        };
        let end = match range.end_bound() {
            Bound::Unbounded => Self::MAX,
            Bound::Included(end) => end.as_data(),
            Bound::Excluded(end) => end.as_data() - Self::ONE,
        };
        if first > end {
            bail!("error:{} > {}", first, end);
        }
        let mut val = self._val();
        while first <= end {
            val |= Self::ONE << first;
            first += Self::ONE;
        }
        self._mut_val(val);
        Ok(self)
    }

    fn to_vec(&self) -> Vec<Self::ValTy> {
        let mut res = Vec::new();
        let val = self._val();
        let mut first = Self::MIN;
        while first <= Self::MAX {
            if (val & (Self::ONE << first)) > Self::ZERO {
                res.push(first);
            }
            first += Self::ONE;
        }
        res
    }
    fn contain<D: AsData<Self::ValTy>>(&self, index: D) -> bool {
        let index = index.as_data();
        let val = self._val();
        val & (Self::ONE << index) > Self::ZERO
    }
    // fn min_self_next<D: AsData<Self::ValTy>>(
    //     &self,
    //     index: D,
    // ) -> (Self::ValTy, Option<Self::ValTy>, Option<Self::ValTy>) {
    //     let self_val = if self.contain(index) {
    //         Some(index.as_data())
    //     } else {
    //         None
    //     };
    //     let min = self.min_val();
    //     let next = self.next(index);
    //     (min, self_val, next)
    // }
    /// 取下一个持有值
    fn next<D: AsData<Self::ValTy>>(&self, index: D) -> Option<Self::ValTy> {
        let mut first = index.as_data() + Self::ONE;
        let val = self._val();
        while first <= Self::MAX {
            if (val & (Self::ONE << first)) > Self::ZERO {
                return Some(first);
            }
            first += Self::ONE;
        }
        None
        // first = Self::MIN;
        // while first <= index.as_data() {
        //     if (val & (Self::ONE << first)) > Self::ZERO {
        //         return Some(first);
        //     }
        //     first += Self::ONE;
        // }
        // unreachable!("it is a bug");
    }
    /// 取最小的持有值
    fn min_val(&self) -> Self::ValTy {
        let mut first = Self::MIN;
        let val = self._val();
        while first <= Self::MAX {
            if (val & (Self::ONE << first)) > Self::ZERO {
                return first;
            }
            first += Self::ONE;
        }
        unreachable!("it is a bug");
    }
    // fn find<D: AsData<Self::ValTy>>(&self, index: D, with_self: bool) -> Self::ValTy {
    //     let index = index.as_data();
    //     let val = self._val();
    //     let index = if with_self { index } else { index + Self::ONE };
    //     let mut first = index;
    //     while first <= Self::MAX {
    //         if (val & (Self::ONE << first)) > Self::ZERO {
    //             return first;
    //         }
    //         first += Self::ONE;
    //     }
    //     first = Self::MIN;
    //     while first < index {
    //         if (val & (Self::ONE << first)) > Self::ZERO {
    //             return first;
    //         }
    //         first += Self::ONE;
    //     }
    //     unreachable!("it is a bug");
    // }
    // #[inline]
    // fn check(index: Self::ValTy) -> Result<()> {
    //     if index >= Self::MIN && index <= Self::MAX {
    //         Ok(())
    //     } else {
    //         bail!("can't out of range {}..={} ", Self::MIN, Self::MAX);
    //     }
    // }
    fn _val(&self) -> Self::ValTy;
    fn _mut_val(&mut self, val: Self::ValTy);
}

// #[derive(Debug, Clone)]
// pub enum Days {
//     MD(MonthDays),
//     WD(WeekDays),
// }
pub struct DayConfBuilder {
    month_days: Option<MonthDays>,
    week_days: Option<WeekDays>,
}
impl DayConfBuilder {
    pub fn conf_month_days(self, month_days: MonthDays) -> Self {
        DayConfBuilder {
            month_days: Some(month_days),
            week_days: self.week_days,
        }
    }
    pub fn conf_week_days(self, week_days: WeekDays) -> Self {
        DayConfBuilder {
            month_days: self.month_days,
            week_days: Some(week_days),
        }
    }
    pub fn build_with_hours(self, hours: Hours) -> DayHourConfBuilder {
        DayHourConfBuilder {
            month_days: self.month_days,
            week_days: self.week_days,
            hours,
        }
    }
}
pub struct DayHourConfBuilder {
    month_days: Option<MonthDays>,
    week_days: Option<WeekDays>,
    hours: Hours,
}
impl DayHourConfBuilder {
    pub fn build_with_minuter_builder(self, minuters: Minuters) -> DayHourMinuterConfBuilder {
        DayHourMinuterConfBuilder {
            month_days: self.month_days,
            week_days: self.week_days,
            hours: self.hours,
            minuters,
        }
    }
}
pub struct DayHourMinuterConfBuilder {
    month_days: Option<MonthDays>,
    week_days: Option<WeekDays>,
    hours: Hours,
    minuters: Minuters,
}
impl DayHourMinuterConfBuilder {
    pub fn build_with_second_builder(self, seconds: Seconds) -> DayHourMinuterSecondConf {
        DayHourMinuterSecondConf {
            month_days: self.month_days,
            week_days: self.week_days,
            hours: self.hours,
            minuters: self.minuters,
            seconds,
        }
    }
}
#[derive(Debug, Clone)]
pub struct DayHourMinuterSecondConf {
    pub(crate) month_days: Option<MonthDays>,
    pub(crate) week_days: Option<WeekDays>,
    pub(crate) hours: Hours,
    pub(crate) minuters: Minuters,
    pub(crate) seconds: Seconds,
}

impl DayHourMinuterSecondConf {
    pub fn default_month_days(month_days: MonthDays) -> DayConfBuilder {
        DayConfBuilder {
            month_days: Some(month_days),
            week_days: None,
        }
    }
    pub fn default_week_days(week_days: WeekDays) -> DayConfBuilder {
        DayConfBuilder {
            month_days: None,
            week_days: Some(week_days),
        }
    }
    pub fn next(&self) -> Result<OffsetDateTime> {
        self._next(DateTime::default()?)
    }
    fn _next(&self, datetime: DateTime) -> Result<OffsetDateTime> {
        let day_self = self
            .month_days
            .as_ref()
            .map_or(false, |x| x.contain(datetime.month_day))
            || self
                .week_days
                .as_ref()
                .map_or(false, |x| x.contain(datetime.week_day));

        let hour_self = self.hours.contain(datetime.hour);
        let minuter_self = self.minuters.contain(datetime.minuter);

        let (mut day_possible, mut hour_possible, mut minuter_possible, mut second_possible) =
            if day_self {
                if hour_self {
                    if minuter_self {
                        (
                            Possible::Oneself,
                            Possible::Oneself,
                            Possible::Oneself,
                            Possible::Next,
                        )
                    } else {
                        (
                            Possible::Oneself,
                            Possible::Oneself,
                            Possible::Next,
                            Possible::Min,
                        )
                    }
                } else {
                    (
                        Possible::Oneself,
                        Possible::Next,
                        Possible::Min,
                        Possible::Min,
                    )
                }
            } else {
                (Possible::Next, Possible::Min, Possible::Min, Possible::Min)
            };
        let (second, second_recount) = get_val(second_possible, &self.seconds, datetime.second);
        if second_recount {
            second_possible = Possible::Min;
            minuter_possible = Possible::Next;
        }
        let (minuter, minuter_recount) =
            get_val(minuter_possible, &self.minuters, datetime.minuter);
        if minuter_recount {
            minuter_possible = Possible::Min;
            hour_possible = Possible::Next;
        }
        let (hour, hour_recount) = get_val(hour_possible, &self.hours, datetime.hour);
        if hour_recount {
            hour_possible = Possible::Min;
            day_possible = Possible::Next;
        }
        let time_next = time::Time::from_hms(hour as u8, minuter as u8, second as u8)?;
        let date_month = if let Some(month_days) = &self.month_days {
            let (month_day, month_day_recount) =
                get_val(day_possible, month_days, datetime.month_day);
            if month_day_recount {
                let mut date = datetime.date.clone();
                date = date.replace_month(date.clone().month().next())?;
                Some(date.replace_day(month_day as u8)?)
            } else {
                let date = datetime.date.clone();
                Some(date.replace_day(month_day as u8)?)
            }
        } else {
            None
        };
        let date_week = if let Some(month_days) = &self.week_days {
            let (week_day, week_day_recount) = get_val(day_possible, month_days, datetime.week_day);
            if week_day_recount {
                let mut date = datetime.date.clone();
                date += Duration::days((week_day + 7 - datetime.week_day.as_data()) as i64);
                Some(date)
            } else {
                let mut date = datetime.date.clone();
                date += Duration::days((week_day - datetime.week_day.as_data()) as i64);
                Some(date)
            }
        } else {
            None
        };
        let date = if let Some(date_month) = date_month {
            if let Some(date_week) = date_week {
                if date_month > date_week {
                    date_week
                } else {
                    date_month
                }
            } else {
                date_month
            }
        } else {
            date_week.unwrap()
        };
        Ok(PrimitiveDateTime::new(date, time_next).assume_utc())
    }
}

pub fn get_val<D: Operator>(
    possible: Possible,
    d: &D,
    oneself: impl AsData<D::ValTy>,
) -> (D::ValTy, bool) {
    let mut re_count = false;
    let data = match possible {
        Possible::Min => d.min_val(),
        Possible::Oneself => oneself.as_data(),
        Possible::Next => {
            if let Some(data) = d.next(oneself) {
                data
            } else {
                re_count = true;
                d.min_val()
            }
        }
    };
    (data, re_count)
}

#[derive(Copy, Clone)]
pub enum Possible {
    Min,
    Oneself,
    Next,
}

impl From<MonthDays> for DayConfBuilder {
    fn from(builder: MonthDays) -> Self {
        DayHourMinuterSecondConf::default_month_days(builder)
    }
}
impl From<WeekDays> for DayConfBuilder {
    fn from(builder: WeekDays) -> Self {
        DayHourMinuterSecondConf::default_week_days(builder)
    }
}
impl Debug for Seconds {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0 == u64::MAX >> 4 {
            write!(f, "all seconds.")
        } else {
            write!(f, "seconds: {:?}.", self.to_vec())
        }
    }
}
impl Debug for Minuters {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0 == u64::MAX >> 4 {
            write!(f, "all minuters.")
        } else {
            write!(f, "minuters: {:?}.", self.to_vec())
        }
    }
}
impl Debug for Hours {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0 == u32::MAX >> 8 {
            write!(f, "all hours.")
        } else {
            write!(f, "hours: {:?}.", self.to_vec())
        }
    }
}
impl Debug for MonthDays {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0 == u32::MAX << 1 {
            write!(f, "all month days.")
        } else {
            write!(f, "month days: {:?}.", self.to_vec())
        }
    }
}
impl Debug for WeekDays {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0 == u8::MAX << 1 {
            write!(f, "all week days.")
        } else {
            write!(f, "week days: {:?}.", self.to_vec())
        }
    }
}
#[cfg(test)]
mod test {
    use super::{DayConfBuilder, Hours, Minuters, MonthDays, Operator, Seconds, WeekDays};
    use crate::util_datetime::data::{
        DateTime, Hour, InnerHour, InnerMinuter, InnerMonthDay, InnerSecond, InnerWeekDay, Minuter,
        MonthDay, Second, WeekDay,
    };
    use crate::util_datetime::{get_val, DayHourMinuterSecondConf, Possible};
    use anyhow::Result;
    use time::Month;

    #[test]
    fn test_get_val() -> Result<()> {
        let some_seconds = Seconds::default_range(Second::S10..Second::S30)?
            .add_range(Second::S40..=Second::S50)?;
        assert_eq!(
            (10, false),
            get_val(Possible::Min, &some_seconds, Second::S40)
        );
        assert_eq!(
            (40, false),
            get_val(Possible::Oneself, &some_seconds, Second::S40)
        );
        assert_eq!(
            (41, false),
            get_val(Possible::Next, &some_seconds, Second::S40)
        );
        assert_eq!(
            (40, false),
            get_val(Possible::Next, &some_seconds, Second::S29)
        );
        assert_eq!(
            (10, true),
            get_val(Possible::Next, &some_seconds, Second::S50)
        );

        Ok(())
    }
    #[test]
    fn test_contain() -> Result<()> {
        let all_seconds = Seconds::default_all();
        assert!(all_seconds.contain(Second::S0));
        assert!(all_seconds.contain(Second::S30));
        assert!(all_seconds.contain(Second::S59));
        let some_seconds = Seconds::default_range(Second::S1..Second::S30)?
            .add_range(Second::S31..=Second::S58)?;
        assert!(!some_seconds.contain(Second::S59));
        assert!(!some_seconds.contain(Second::S0));
        assert!(!some_seconds.contain(Second::S30));

        let all_hours = Hours::default_all();
        assert!(all_hours.contain(Hour::H10));
        assert!(all_hours.contain(Hour::H0));
        assert!(all_hours.contain(Hour::H23));
        let some_hours =
            Hours::default_range(Hour::H1..Hour::H10)?.add_range(Hour::H11..=Hour::H22)?;
        assert!(!some_hours.contain(Hour::H10));
        assert!(!some_hours.contain(Hour::H0));
        assert!(!some_hours.contain(Hour::H23));

        let all_weeks = WeekDays::default_all();
        assert!(all_weeks.contain(WeekDay::W3));
        assert!(all_weeks.contain(WeekDay::W1));
        assert!(all_weeks.contain(WeekDay::W7));
        let some_weeks = WeekDays::default_range(WeekDay::W2..WeekDay::W3)?
            .add_range(WeekDay::W4..=WeekDay::W6)?;
        assert!(!some_weeks.contain(WeekDay::W3));
        assert!(!some_weeks.contain(WeekDay::W1));
        assert!(!some_weeks.contain(WeekDay::W7));
        Ok(())
    }
    #[test]
    fn test() -> anyhow::Result<()> {
        let conf = DayHourMinuterSecondConf::default_week_days(WeekDays::default_array(&[
            WeekDay::W5,
            WeekDay::W3,
        ]))
        .conf_month_days(MonthDays::default_array(&[
            MonthDay::M5,
            MonthDay::M15,
            MonthDay::M24,
        ]))
        .build_with_hours(Hours::default_array(&[Hour::H5, Hour::H10, Hour::H15]))
        .build_with_minuter_builder(Minuters::default_array(&[
            Minuter::M15,
            Minuter::M30,
            Minuter::M45,
        ]))
        .build_with_second_builder(Seconds::default_array(&[
            Second::S15,
            Second::S30,
            Second::S45,
        ]));

        let mut dt0 = DateTime {
            date: time::Date::from_calendar_date(2022, Month::May, 15)?,
            month_day: InnerMonthDay(15),
            week_day: InnerWeekDay(7),
            hour: InnerHour(10),
            minuter: InnerMinuter(30),
            second: InnerSecond(30),
        };

        {
            let dist: DateTime = conf._next(dt0)?.into();
            let mut dt0_dist = dt0.clone();
            dt0_dist.second = InnerSecond(45);
            assert!(dist == dt0_dist);
            dt0_dist.second = InnerSecond(31);
            assert!(dist != dt0_dist);
        }
        //
        {
            dt0.second = InnerSecond(45);
            let dist: DateTime = conf._next(dt0)?.into();
            let mut dt0_dist = dt0.clone();
            dt0_dist.second = InnerSecond(15);
            dt0_dist.minuter = InnerMinuter(45);
            assert!(dist == dt0_dist);
        }
        {
            dt0.second = InnerSecond(45);
            dt0.minuter = InnerMinuter(45);
            let dist: DateTime = conf._next(dt0)?.into();
            let mut dt0_dist = dt0.clone();
            dt0_dist.second = InnerSecond(15);
            dt0_dist.minuter = InnerMinuter(15);
            dt0_dist.hour = InnerHour(15);
            assert!(dist == dt0_dist);
        }
        {
            dt0.second = InnerSecond(45);
            dt0.minuter = InnerMinuter(45);
            dt0.hour = InnerHour(15);
            let dist: DateTime = conf._next(dt0)?.into();
            let mut dt0_dist = dt0.clone();
            dt0_dist.second = InnerSecond(15);
            dt0_dist.minuter = InnerMinuter(15);
            dt0_dist.hour = InnerHour(5);
            dt0_dist.week_day = InnerWeekDay(3);
            dt0_dist.month_day = InnerMonthDay(18);
            dt0_dist.date = time::Date::from_calendar_date(2022, Month::May, 18)?;
            assert_eq!(dist, dt0_dist);
        }

        let mut dt0 = DateTime {
            date: time::Date::from_calendar_date(2022, Month::May, 20)?,
            month_day: InnerMonthDay(20),
            week_day: InnerWeekDay(5),
            hour: InnerHour(15),
            minuter: InnerMinuter(45),
            second: InnerSecond(45),
        };
        {
            let dist: DateTime = conf._next(dt0)?.into();
            let mut dt0_dist = dt0.clone();
            dt0_dist.week_day = InnerWeekDay(2);
            dt0_dist.month_day = InnerMonthDay(24);
            dt0_dist.second = InnerSecond(15);
            dt0_dist.minuter = InnerMinuter(15);
            dt0_dist.hour = InnerHour(5);
            dt0_dist.date = time::Date::from_calendar_date(2022, Month::May, 24)?;
            assert_eq!(dist, dt0_dist);
        }
        Ok(())
    }
    #[test]
    fn test_week_days() -> anyhow::Result<()> {
        let wds = WeekDays::default_value(WeekDay::W1).add_range(WeekDay::W3..=WeekDay::W5)?;
        let days = wds.to_vec();
        assert_eq!(vec![1, 3, 4, 5], days);
        // assert_eq!(1, wds.find(WeekDay::W1, true));
        // assert_eq!(3, wds.find(WeekDay::W1, false));
        // assert_eq!(1, wds.find(WeekDay::W6, false));

        let wds = WeekDays::default_value(WeekDay::W1).add_array(&[WeekDay::W3, WeekDay::W5]);
        let days = wds.to_vec();
        assert_eq!(vec![1, 3, 5], days);
        // assert_eq!(1, wds.find(WeekDay::W1, true));
        // assert_eq!(3, wds.find(WeekDay::W1, false));
        // assert_eq!(1, wds.find(WeekDay::W6, false));

        let days = WeekDays::default_all().to_vec();
        assert_eq!(vec![1, 2, 3, 4, 5, 6, 7], days);
        assert!(WeekDays::default_range(WeekDay::W5..WeekDay::W5).is_err());
        assert!(WeekDays::default_range(WeekDay::W5..WeekDay::W1).is_err());
        let days = WeekDays::default_range(WeekDay::W5..WeekDay::W6)?.to_vec();
        assert_eq!(vec![5], days);

        Ok(())
    }
}
