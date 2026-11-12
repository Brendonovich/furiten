use super::*;

const T0: i64 = 1_800_000_000_000;

#[test]
fn new_card_is_due_at_creation() {
    let s = Schedule::new(T0);
    assert_eq!(s.state, State::New);
    assert_eq!(s.due, T0);
}

#[test]
fn good_on_new_card_graduates() {
    let fsrs = Scheduler::default();
    let s = fsrs.review(&Schedule::new(T0), Rating::Good, T0);
    assert_eq!(s.state, State::Review);
    assert_eq!(s.due, T0 + fsrs.interval_days(DEFAULT_WEIGHTS[2]) as i64 * DAY);
}

#[test]
fn easy_on_new_card_graduates_further_out() {
    let fsrs = Scheduler::default();
    let s = fsrs.review(&Schedule::new(T0), Rating::Easy, T0);
    assert_eq!(s.state, State::Review);
    assert_eq!(s.due, T0 + fsrs.interval_days(DEFAULT_WEIGHTS[3]) as i64 * DAY);
}

#[test]
fn again_on_new_card_is_due_at_once_until_remembered() {
    let fsrs = Scheduler::default();
    let a = fsrs.review(&Schedule::new(T0), Rating::Again, T0);
    assert_eq!((a.state, a.due), (State::Learning, T0));
    let b = fsrs.review(&a, Rating::Again, T0 + 1000);
    assert_eq!((b.state, b.due), (State::Learning, T0 + 1000));
    let c = fsrs.review(&b, Rating::Good, T0 + 2000);
    assert_eq!(c.state, State::Review);
    assert!(c.due >= T0 + 2000 + DAY);
}

#[test]
fn intervals_grow_with_successful_reviews() {
    let fsrs = Scheduler::default();
    let mut s = fsrs.review(&Schedule::new(T0), Rating::Easy, T0);
    let mut last_interval = 0;
    for _ in 0..5 {
        let interval = s.due - s.last_review.unwrap();
        assert!(interval > last_interval, "{interval} <= {last_interval}");
        last_interval = interval;
        s = fsrs.review(&s, Rating::Good, s.due);
    }
}

#[test]
fn again_in_review_lapses_and_lowers_stability() {
    let fsrs = Scheduler::default();
    let s = fsrs.review(&Schedule::new(T0), Rating::Easy, T0);
    let lapsed = fsrs.review(&s, Rating::Again, s.due);
    assert_eq!(lapsed.state, State::Relearning);
    assert_eq!(lapsed.lapses, 1);
    assert!(lapsed.stability < s.stability);
    assert_eq!(lapsed.due, s.due);
    let relearned = fsrs.review(&lapsed, Rating::Good, s.due + 1000);
    assert_eq!(relearned.state, State::Review);
    assert_eq!(relearned.lapses, 1);
}

#[test]
fn ratings_order_difficulty() {
    let fsrs = Scheduler::default();
    let d: Vec<f64> = [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy]
        .iter()
        .map(|r| fsrs.review(&Schedule::new(T0), *r, T0).difficulty)
        .collect();
    assert!(d.windows(2).all(|p| p[0] > p[1]), "{d:?}");
}

#[test]
fn fold_matches_step_by_step() {
    let fsrs = Scheduler::default();
    let reviews =
        [(T0, Rating::Good), (T0 + 60_000, Rating::Again), (T0 + 120_000, Rating::Good), (T0 + 3 * DAY, Rating::Hard)];
    let stepped = reviews.iter().fold(Schedule::new(T0), |s, (at, r)| fsrs.review(&s, *r, *at));
    assert_eq!(fsrs.fold(T0, reviews), stepped);
}

#[test]
fn retrievability_is_retention_at_due() {
    let fsrs = Scheduler::default();
    let s = fsrs.review(&Schedule::new(T0), Rating::Easy, T0);
    let r = fsrs.retrievability(&s, s.due);
    assert!((r - 0.9).abs() < 0.03, "{r}");
}
