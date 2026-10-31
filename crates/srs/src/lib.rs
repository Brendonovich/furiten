//! FSRS-6 with its published default parameters. Deterministic (no fuzz) so every device and the server derive the
//! same schedule from the same reviews.

#[cfg(target_arch = "wasm32")]
mod wasm;

const MINUTE: i64 = 60_000;
const DAY: i64 = 86_400_000;
const MAX_INTERVAL_DAYS: f64 = 36_500.0;

pub const DEFAULT_WEIGHTS: [f64; 21] = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835, 0.0614, 0.2629,
    1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    New,
    Learning,
    Review,
    Relearning,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::New => "new",
            State::Learning => "learning",
            State::Review => "review",
            State::Relearning => "relearning",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

impl Rating {
    pub fn from_u8(value: u8) -> Option<Rating> {
        match value {
            1 => Some(Rating::Again),
            2 => Some(Rating::Hard),
            3 => Some(Rating::Good),
            4 => Some(Rating::Easy),
            _ => None,
        }
    }

    fn g(self) -> f64 {
        self as u8 as f64
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Schedule {
    pub state: State,
    /// Unix ms.
    pub due: i64,
    pub stability: f64,
    pub difficulty: f64,
    pub reps: u32,
    pub lapses: u32,
    pub last_review: Option<i64>,
}

impl Schedule {
    pub fn new(created_at: i64) -> Schedule {
        Schedule {
            state: State::New,
            due: created_at,
            stability: 0.0,
            difficulty: 0.0,
            reps: 0,
            lapses: 0,
            last_review: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Scheduler {
    pub w: [f64; 21],
    pub retention: f64,
}

impl Default for Scheduler {
    fn default() -> Scheduler {
        Scheduler { w: DEFAULT_WEIGHTS, retention: 0.9 }
    }
}

impl Scheduler {
    /// Folds reviews (already ordered by `(reviewed_at, id)`) over a new card.
    pub fn fold(&self, created_at: i64, reviews: impl IntoIterator<Item = (i64, Rating)>) -> Schedule {
        reviews.into_iter().fold(Schedule::new(created_at), |s, (at, rating)| self.review(&s, rating, at))
    }

    pub fn review(&self, s: &Schedule, rating: Rating, now: i64) -> Schedule {
        let reps = s.reps + 1;
        if s.state == State::New {
            let stability = self.init_stability(rating);
            let difficulty = self.init_difficulty(rating);
            return match rating {
                Rating::Easy => self.graduate(stability, difficulty, reps, s.lapses, now),
                _ => Schedule {
                    state: State::Learning,
                    due: now + step(rating, State::Learning),
                    stability,
                    difficulty,
                    reps,
                    lapses: s.lapses,
                    last_review: Some(now),
                },
            };
        }

        let elapsed_days = s.last_review.map_or(0.0, |last| ((now - last).max(0) / DAY) as f64);
        let difficulty = self.next_difficulty(s.difficulty, rating);
        let same_day = elapsed_days < 1.0;
        let r = self.retrievability_days(elapsed_days, s.stability);

        if s.state == State::Review {
            if rating == Rating::Again {
                let stability = if same_day {
                    self.short_term_stability(s.stability, rating)
                } else {
                    self.forget_stability(s.difficulty, s.stability, r)
                };
                return Schedule {
                    state: State::Relearning,
                    due: now + step(rating, State::Relearning),
                    stability,
                    difficulty,
                    reps,
                    lapses: s.lapses + 1,
                    last_review: Some(now),
                };
            }
            let stability = if same_day {
                self.short_term_stability(s.stability, rating)
            } else {
                self.recall_stability(s.difficulty, s.stability, r, rating)
            };
            return self.graduate(stability, difficulty, reps, s.lapses, now);
        }

        // Learning or relearning.
        let stability = if same_day {
            self.short_term_stability(s.stability, rating)
        } else if rating == Rating::Again {
            self.forget_stability(s.difficulty, s.stability, r)
        } else {
            self.recall_stability(s.difficulty, s.stability, r, rating)
        };
        match rating {
            Rating::Again | Rating::Hard => Schedule {
                state: s.state,
                due: now + step(rating, s.state),
                stability,
                difficulty,
                reps,
                lapses: s.lapses,
                last_review: Some(now),
            },
            _ => self.graduate(stability, difficulty, reps, s.lapses, now),
        }
    }

    /// Probability of recall at `now`, 0..=1. New cards return 0.
    pub fn retrievability(&self, s: &Schedule, now: i64) -> f64 {
        match s.last_review {
            Some(last) if s.state != State::New => {
                self.retrievability_days((now - last).max(0) as f64 / DAY as f64, s.stability)
            }
            _ => 0.0,
        }
    }

    /// Interval in whole days for a stability at the desired retention.
    pub fn interval_days(&self, stability: f64) -> f64 {
        let decay = self.decay();
        let interval = stability / self.factor() * (self.retention.powf(1.0 / decay) - 1.0);
        interval.round().clamp(1.0, MAX_INTERVAL_DAYS)
    }

    fn graduate(&self, stability: f64, difficulty: f64, reps: u32, lapses: u32, now: i64) -> Schedule {
        Schedule {
            state: State::Review,
            due: now + self.interval_days(stability) as i64 * DAY,
            stability,
            difficulty,
            reps,
            lapses,
            last_review: Some(now),
        }
    }

    fn decay(&self) -> f64 {
        -self.w[20]
    }

    fn factor(&self) -> f64 {
        0.9_f64.powf(1.0 / self.decay()) - 1.0
    }

    fn retrievability_days(&self, elapsed_days: f64, stability: f64) -> f64 {
        (1.0 + self.factor() * elapsed_days / stability).powf(self.decay())
    }

    fn init_stability(&self, rating: Rating) -> f64 {
        self.w[rating as usize - 1].max(0.001)
    }

    fn init_difficulty(&self, rating: Rating) -> f64 {
        self.raw_init_difficulty(rating).clamp(1.0, 10.0)
    }

    fn raw_init_difficulty(&self, rating: Rating) -> f64 {
        self.w[4] - (self.w[5] * (rating.g() - 1.0)).exp() + 1.0
    }

    fn next_difficulty(&self, d: f64, rating: Rating) -> f64 {
        let delta = -self.w[6] * (rating.g() - 3.0);
        let damped = d + delta * (10.0 - d) / 9.0;
        let reverted = self.w[7] * self.raw_init_difficulty(Rating::Easy) + (1.0 - self.w[7]) * damped;
        reverted.clamp(1.0, 10.0)
    }

    fn recall_stability(&self, d: f64, s: f64, r: f64, rating: Rating) -> f64 {
        let hard = if rating == Rating::Hard { self.w[15] } else { 1.0 };
        let easy = if rating == Rating::Easy { self.w[16] } else { 1.0 };
        let w = &self.w;
        s * (w[8].exp() * (11.0 - d) * s.powf(-w[9]) * ((w[10] * (1.0 - r)).exp() - 1.0) * hard * easy + 1.0)
    }

    fn forget_stability(&self, d: f64, s: f64, r: f64) -> f64 {
        let w = &self.w;
        let forgot = w[11] * d.powf(-w[12]) * ((s + 1.0).powf(w[13]) - 1.0) * (w[14] * (1.0 - r)).exp();
        forgot.min(s / (w[17] * w[18]).exp())
    }

    fn short_term_stability(&self, s: f64, rating: Rating) -> f64 {
        let w = &self.w;
        let inc = (w[17] * (rating.g() - 3.0 + w[18])).exp() * s.powf(-w[19]);
        let inc = if rating.g() >= 3.0 { inc.max(1.0) } else { inc };
        s * inc
    }
}

fn step(rating: Rating, state: State) -> i64 {
    match (state, rating) {
        (State::Relearning, _) => 10 * MINUTE,
        (_, Rating::Again) => MINUTE,
        (_, Rating::Hard) => 5 * MINUTE,
        _ => 10 * MINUTE,
    }
}

#[cfg(test)]
mod tests;
