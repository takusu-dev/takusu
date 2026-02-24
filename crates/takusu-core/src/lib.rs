use jiff::Timestamp;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point(i64);

impl Point {
    pub fn from_timestamp(ts: Timestamp, per: u16) -> Point {
        Point(ts.as_second() / per as i64 / 60)
    }

    pub fn diff(lhs: Point, rhs: Point) -> i64 {
        (lhs.0 - rhs.0).abs()
    }

    pub fn now(per: u16) -> Self {
        Self::from_timestamp(Timestamp::now(), per)
    }
}

/// start <= task < end
#[derive(Debug, Clone)]
pub struct Task {
    pub id: usize, // takusu-serveで適宜nanoidと変換してもらう

    pub start: Option<Point>,
    pub end: Point,

    pub cost_estimate: NormalDist, // 初期の実装ではavgだけ考えればいい

    pub depends: Vec<usize>,
    // TODO: pluginable, parallelとか
}

impl Task {}

#[derive(Debug, Clone)]
pub struct NormalDist {
    pub avg: u64,
    pub sigma: u64,
}

#[derive(Debug)]
pub struct Planner {
    tasks: Vec<Task>,
    now: Point,
}

#[derive(Debug)]
pub struct Plan {
    // start <= (task) <= end , id
    pub schedules: Vec<(Point, Point, usize)>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("The start is {0:?} but the end is {1:?} which is earlier than the start")]
    LateStart(Point, Point),
}

type ResultE<T> = Result<T, Error>;

// 基本的に焼きなまし法?
// 期限が過ぎているやつがあったら最優先
// freenessを計算して低いやつから
// あとグラフを解析する
impl Planner {
    pub fn new(now: Point) -> Self {
        Self { tasks: vec![], now }
    }

    // the higher, the freer(so can be delayed)
    fn freeness(&self, id: usize) -> f64 {
        1. - (self.tasks[id].cost_estimate.avg as f64
            / Point::diff(
                self.tasks[id]
                    .start
                    .or(Some(Point(0)))
                    .unwrap()
                    .max(self.now),
                self.tasks[id].end,
            ) as f64)
    }

    pub fn add_task(
        &mut self,
        start: Option<Point>,
        end: Point,
        cost_estimate: NormalDist,
        depends: Vec<usize>,
    ) -> ResultE<usize> {
        let id = self.tasks.len();

        if let Some(start) = start
            && start > end
        {
            return Err(Error::LateStart(start, end));
        }

        self.tasks.push(Task {
            id,
            start,
            end,
            cost_estimate,
            depends,
        });

        Ok(id)
    }

    pub fn plan(&self) -> Plan {
        todo!()
    }
}
