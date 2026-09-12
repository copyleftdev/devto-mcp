//! Client-side pacing against Forem's throttles.
//!
//! dev.to returns no `X-RateLimit-*` headers — verified against a live authenticated
//! request — so a client that wants to stay inside the budget has to model it locally.
//!
//! `config/initializers/rack_attack.rb` applies six throttles to `/api/*`, and the key
//! detail is that they count **per IP and per api-key at the same time**. Two processes on
//! one machine share the IP budget; one key used from two machines shares the key budget.
//! Modelling them as one set of buckets is the conservative reading, and the only safe one.
//!
//! Rack::Attack uses fixed windows; this is a sliding window, which is stricter. Being
//! stricter is free — the alternative is a 429 that costs a request and a `Retry-After` wait.

use std::collections::VecDeque;

/// Milliseconds from an arbitrary monotonic origin. The limiter never reads a clock.
pub type Millis = u64;

/// Which budget a request draws on. Forem splits on the HTTP method, not the endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    Read,
    Write,
}

/// `api_throttle` / `api_key_throttle`: 3 GET per second.
pub const READ_BURST_LIMIT: usize = 3;
pub const READ_BURST_WINDOW_MS: Millis = 1_000;

/// `api_throttle_per_minute` / `api_key_throttle_per_minute`: 30 GET per minute.
/// This is the binding constraint in practice.
pub const READ_SUSTAINED_LIMIT: usize = 30;
pub const READ_SUSTAINED_WINDOW_MS: Millis = 60_000;

/// `api_write_throttle` / `api_write_key_throttle`: 1 write per second.
pub const WRITE_LIMIT: usize = 1;
pub const WRITE_WINDOW_MS: Millis = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The request fits in the budget now.
    Proceed,
    /// Wait this long and the request will fit.
    Wait(Millis),
}

impl Decision {
    pub fn wait_millis(self) -> Millis {
        match self {
            Self::Proceed => 0,
            Self::Wait(ms) => ms,
        }
    }
}

/// What is left in each bucket, for reporting to a caller that wants to plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub reads_this_second: usize,
    pub reads_this_minute: usize,
    pub writes_this_second: usize,
}

#[derive(Debug, Clone)]
struct Window {
    limit: usize,
    span: Millis,
    hits: VecDeque<Millis>,
}

impl Window {
    fn new(limit: usize, span: Millis) -> Self {
        Self {
            limit,
            span,
            hits: VecDeque::new(),
        }
    }

    fn expire(&mut self, now: Millis) {
        while let Some(&oldest) = self.hits.front() {
            if now.saturating_sub(oldest) >= self.span {
                self.hits.pop_front();
            } else {
                break;
            }
        }
    }

    /// How long until this window has room, without mutating it.
    ///
    /// The liveness test is applied once and the result reused: counting with one
    /// predicate and then indexing with a second copy of it is how an off-by-one gets in,
    /// and the two would disagree for a hit sitting exactly on the span boundary.
    fn wait_for(&self, now: Millis) -> Millis {
        let live = self.live_hits(now);
        if live.len() < self.limit {
            return 0;
        }
        // Slots free in order, so the one that unblocks us is `limit` from the end.
        let blocking = live[live.len() - self.limit];
        self.span - now.saturating_sub(blocking)
    }

    fn live_hits(&self, now: Millis) -> Vec<Millis> {
        self.hits
            .iter()
            .copied()
            .filter(|&t| now.saturating_sub(t) < self.span)
            .collect()
    }

    fn record(&mut self, now: Millis) {
        self.expire(now);
        self.hits.push_back(now);
    }

    fn live(&self, now: Millis) -> usize {
        self.live_hits(now).len()
    }
}

/// Tracks what has been sent and says when the next request may go.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    read_burst: Window,
    read_sustained: Window,
    write: Window,
    /// Set by a server-side 429. Held separately rather than by stuffing the windows with
    /// back-dated hits: a hit that should expire at `until` would have to sit at
    /// `until - span`, which is not representable once `until` is inside one window span.
    blocked_until: Millis,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            read_burst: Window::new(READ_BURST_LIMIT, READ_BURST_WINDOW_MS),
            read_sustained: Window::new(READ_SUSTAINED_LIMIT, READ_SUSTAINED_WINDOW_MS),
            write: Window::new(WRITE_LIMIT, WRITE_WINDOW_MS),
            blocked_until: 0,
        }
    }

    /// How long to wait before a request of this kind may be sent.
    pub fn check(&self, kind: RequestKind, now: Millis) -> Decision {
        let from_windows = match kind {
            RequestKind::Read => self
                .read_burst
                .wait_for(now)
                .max(self.read_sustained.wait_for(now)),
            RequestKind::Write => self.write.wait_for(now),
        };
        let wait = from_windows.max(self.blocked_until.saturating_sub(now));
        if wait == 0 {
            Decision::Proceed
        } else {
            Decision::Wait(wait)
        }
    }

    /// Charge the budget. Call this when a request is actually sent, whatever it returns —
    /// a 401 or a 422 costs exactly as much throttle as a 200 does.
    pub fn record(&mut self, kind: RequestKind, now: Millis) {
        match kind {
            RequestKind::Read => {
                self.read_burst.record(now);
                self.read_sustained.record(now);
            }
            RequestKind::Write => self.write.record(now),
        }
    }

    /// A server-side 429 means our model was wrong, or someone else is spending the same
    /// budget. Trust the server: nothing of any kind goes out until its window has passed.
    pub fn penalize(&mut self, now: Millis, retry_after_millis: Millis) {
        self.blocked_until = self.blocked_until.max(now + retry_after_millis);
    }

    /// Whether a server-side penalty is still in force.
    pub fn is_penalized(&self, now: Millis) -> bool {
        self.blocked_until > now
    }

    /// How many timestamps are being retained. Bounded retention is the reason `expire`
    /// exists: the windows are consulted by filtering, so a limiter that never forgot
    /// would still answer correctly while growing for as long as the process runs.
    pub fn tracked_hits(&self) -> usize {
        self.read_burst.hits.len() + self.read_sustained.hits.len() + self.write.hits.len()
    }

    pub fn budget(&self, now: Millis) -> Budget {
        Budget {
            reads_this_second: READ_BURST_LIMIT.saturating_sub(self.read_burst.live(now)),
            reads_this_minute: READ_SUSTAINED_LIMIT.saturating_sub(self.read_sustained.live(now)),
            writes_this_second: WRITE_LIMIT.saturating_sub(self.write.live(now)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_limiter_lets_the_first_request_through() {
        let limiter = RateLimiter::new();
        assert_eq!(limiter.check(RequestKind::Read, 0), Decision::Proceed);
        assert_eq!(limiter.check(RequestKind::Write, 0), Decision::Proceed);
    }

    #[test]
    fn the_read_burst_is_three_per_second() {
        let mut limiter = RateLimiter::new();
        for _ in 0..READ_BURST_LIMIT {
            assert_eq!(limiter.check(RequestKind::Read, 0), Decision::Proceed);
            limiter.record(RequestKind::Read, 0);
        }
        assert_eq!(
            limiter.check(RequestKind::Read, 0),
            Decision::Wait(READ_BURST_WINDOW_MS)
        );
        assert_eq!(
            limiter.check(RequestKind::Read, READ_BURST_WINDOW_MS - 1),
            Decision::Wait(1)
        );
        assert_eq!(
            limiter.check(RequestKind::Read, READ_BURST_WINDOW_MS),
            Decision::Proceed
        );
    }

    /// Thirty a minute is the constraint that actually bites: a client can sit under three
    /// per second all day and still run out.
    #[test]
    fn the_sustained_read_limit_is_thirty_per_minute() {
        let mut limiter = RateLimiter::new();
        let mut now = 0;
        for _ in 0..READ_SUSTAINED_LIMIT {
            assert_eq!(limiter.check(RequestKind::Read, now), Decision::Proceed);
            limiter.record(RequestKind::Read, now);
            now += 400; // comfortably inside the burst limit
        }
        assert!(matches!(
            limiter.check(RequestKind::Read, now),
            Decision::Wait(_)
        ));
        assert_eq!(
            limiter.check(RequestKind::Read, READ_SUSTAINED_WINDOW_MS),
            Decision::Proceed
        );
    }

    #[test]
    fn writes_are_one_per_second_and_do_not_touch_the_read_budget() {
        let mut limiter = RateLimiter::new();
        limiter.record(RequestKind::Write, 0);
        assert_eq!(
            limiter.check(RequestKind::Write, 0),
            Decision::Wait(WRITE_WINDOW_MS)
        );
        assert_eq!(limiter.check(RequestKind::Read, 0), Decision::Proceed);
        assert_eq!(
            limiter.check(RequestKind::Write, WRITE_WINDOW_MS),
            Decision::Proceed
        );
    }

    #[test]
    fn reads_do_not_consume_the_write_budget() {
        let mut limiter = RateLimiter::new();
        for i in 0..READ_BURST_LIMIT {
            limiter.record(RequestKind::Read, i as Millis);
        }
        assert_eq!(limiter.check(RequestKind::Write, 0), Decision::Proceed);
    }

    #[test]
    fn the_budget_report_counts_down_and_recovers() {
        let mut limiter = RateLimiter::new();
        assert_eq!(
            limiter.budget(0),
            Budget {
                reads_this_second: 3,
                reads_this_minute: 30,
                writes_this_second: 1,
            }
        );

        limiter.record(RequestKind::Read, 0);
        limiter.record(RequestKind::Write, 0);
        assert_eq!(
            limiter.budget(0),
            Budget {
                reads_this_second: 2,
                reads_this_minute: 29,
                writes_this_second: 0,
            }
        );

        assert_eq!(
            limiter.budget(READ_SUSTAINED_WINDOW_MS),
            Budget {
                reads_this_second: 3,
                reads_this_minute: 30,
                writes_this_second: 1,
            }
        );
    }

    /// A 429 means the model disagreed with the server. Trust the server.
    #[test]
    fn a_penalty_stops_everything_until_the_retry_window_passes() {
        let mut limiter = RateLimiter::new();
        limiter.penalize(0, 5_000);

        assert_eq!(limiter.check(RequestKind::Read, 0), Decision::Wait(5_000));
        assert_eq!(limiter.check(RequestKind::Write, 0), Decision::Wait(5_000));
        assert_eq!(limiter.check(RequestKind::Read, 4_999), Decision::Wait(1));
        assert_eq!(limiter.check(RequestKind::Read, 5_000), Decision::Proceed);
        assert_eq!(limiter.check(RequestKind::Write, 5_000), Decision::Proceed);
    }

    #[test]
    fn the_window_constants_match_rack_attack() {
        assert_eq!(READ_BURST_LIMIT, 3);
        assert_eq!(READ_BURST_WINDOW_MS, 1_000);
        assert_eq!(READ_SUSTAINED_LIMIT, 30);
        assert_eq!(READ_SUSTAINED_WINDOW_MS, 60_000);
        assert_eq!(WRITE_LIMIT, 1);
        assert_eq!(WRITE_WINDOW_MS, 1_000);
    }

    /// A hit exactly one window old has left the window. Getting this boundary wrong by one
    /// either wastes a slot forever or spends one we do not have.
    #[test]
    fn a_hit_exactly_one_window_old_no_longer_counts() {
        let mut limiter = RateLimiter::new();
        limiter.record(RequestKind::Write, 0);
        assert_eq!(limiter.budget(WRITE_WINDOW_MS - 1).writes_this_second, 0);
        assert_eq!(limiter.budget(WRITE_WINDOW_MS).writes_this_second, 1);
        assert_eq!(
            limiter.check(RequestKind::Write, WRITE_WINDOW_MS),
            Decision::Proceed
        );
    }

    #[test]
    fn a_penalty_is_reported_until_it_lapses() {
        let mut limiter = RateLimiter::new();
        assert!(!limiter.is_penalized(0));

        limiter.penalize(0, 5_000);
        assert!(limiter.is_penalized(0));
        assert!(limiter.is_penalized(4_999));
        assert!(!limiter.is_penalized(5_000));
        assert!(!limiter.is_penalized(9_000));
    }

    /// A longer penalty wins; a shorter one must not shorten a block already in force.
    #[test]
    fn penalties_extend_but_never_shorten() {
        let mut limiter = RateLimiter::new();
        limiter.penalize(0, 10_000);
        limiter.penalize(0, 1_000);
        assert!(
            limiter.is_penalized(5_000),
            "a shorter penalty cut the block short"
        );

        limiter.penalize(0, 20_000);
        assert!(limiter.is_penalized(15_000));
    }

    /// The windows answer by filtering, so a limiter that never expired anything would
    /// still give correct answers — while growing for as long as the process runs. What
    /// has to hold is that retention is a function of the window spans, not of how many
    /// requests have been made.
    #[test]
    fn retained_timestamps_do_not_grow_with_the_number_of_requests() {
        let record_n = |n: u64| {
            let mut limiter = RateLimiter::new();
            for i in 0..n {
                limiter.record(RequestKind::Read, i * 1_000);
                limiter.record(RequestKind::Write, i * 1_000);
            }
            limiter.tracked_hits()
        };

        let after_1k = record_n(1_000);
        let after_10k = record_n(10_000);
        assert_eq!(
            after_1k, after_10k,
            "retention grew from {after_1k} to {after_10k} with ten times the requests"
        );

        // One hit per second against a sixty second span: the sustained window is the one
        // that retains anything worth counting.
        assert!(
            after_10k <= READ_SUSTAINED_WINDOW_MS as usize / 1_000 + READ_BURST_LIMIT + WRITE_LIMIT,
            "retained {after_10k} timestamps"
        );
    }

    /// Retention is what `tracked_hits` reports, and a read is held by both read windows
    /// while a write is held by one.
    #[test]
    fn tracked_hits_counts_every_window_that_still_holds_something() {
        let mut limiter = RateLimiter::new();
        assert_eq!(limiter.tracked_hits(), 0);

        limiter.record(RequestKind::Read, 0);
        assert_eq!(
            limiter.tracked_hits(),
            2,
            "a read sits in both read windows"
        );

        limiter.record(RequestKind::Write, 0);
        assert_eq!(limiter.tracked_hits(), 3);

        // Two seconds on, the burst window has forgotten its hit and the minute window
        // has not; the write window has seen nothing since, so it still holds its own.
        limiter.record(RequestKind::Read, 2_000);
        assert_eq!(limiter.tracked_hits(), 4);
    }

    /// A hit sitting exactly on the span boundary has left the window, so it can never be
    /// the hit we are waiting on. Treating it as live would report no wait at all.
    #[test]
    fn a_hit_that_has_just_aged_out_is_not_the_one_we_wait_for() {
        let mut limiter = RateLimiter::new();
        for t in [0, 500, 600, 700] {
            limiter.record(RequestKind::Read, t);
        }
        assert_eq!(
            limiter.check(RequestKind::Read, 1_000),
            Decision::Wait(500),
            "the slot frees when the 500ms hit expires, not the one already gone"
        );
    }

    fn any_schedule(tc: &hegel::TestCase) -> Vec<(RequestKind, Millis)> {
        let count = tc.draw(
            hegel::generators::integers::<usize>()
                .min_value(0)
                .max_value(80),
        );
        let mut now: Millis = 0;
        let mut out = Vec::new();
        for _ in 0..count {
            now += tc.draw(
                hegel::generators::integers::<Millis>()
                    .min_value(0)
                    .max_value(3_000),
            );
            let kind = if tc.draw(hegel::generators::booleans()) {
                RequestKind::Read
            } else {
                RequestKind::Write
            };
            out.push((kind, now));
        }
        out
    }

    /// The property the whole file exists for: a client that always waits as long as the
    /// limiter says never exceeds any of Forem's three ceilings. Anything less and the
    /// pacing is decoration.
    #[hegel::test]
    fn obeying_the_limiter_never_exceeds_forems_ceilings(tc: hegel::TestCase) {
        let mut limiter = RateLimiter::new();
        let mut reads: Vec<Millis> = Vec::new();
        let mut writes: Vec<Millis> = Vec::new();

        let mut clock: Millis = 0;
        for (kind, requested_at) in any_schedule(&tc) {
            // A real caller's clock never runs backwards: the next request is asked for no
            // earlier than the previous one was sent.
            let requested_at = requested_at.max(clock);
            let sent_at = requested_at + limiter.check(kind, requested_at).wait_millis();
            clock = sent_at;
            assert_eq!(
                limiter.check(kind, sent_at),
                Decision::Proceed,
                "the limiter asked for a wait that was not long enough"
            );
            limiter.record(kind, sent_at);
            match kind {
                RequestKind::Read => reads.push(sent_at),
                RequestKind::Write => writes.push(sent_at),
            }
        }

        let max_in = |times: &[Millis], span: Millis| -> usize {
            times
                .iter()
                .map(|&t| times.iter().filter(|&&u| u <= t && t - u < span).count())
                .max()
                .unwrap_or(0)
        };

        assert!(
            max_in(&reads, READ_BURST_WINDOW_MS) <= READ_BURST_LIMIT,
            "exceeded 3 reads/sec"
        );
        assert!(
            max_in(&reads, READ_SUSTAINED_WINDOW_MS) <= READ_SUSTAINED_LIMIT,
            "exceeded 30 reads/min"
        );
        assert!(
            max_in(&writes, WRITE_WINDOW_MS) <= WRITE_LIMIT,
            "exceeded 1 write/sec"
        );
    }

    /// Waiting must be the shortest wait that works: a limiter that always says "wait a
    /// minute" would satisfy the ceiling property while making the client useless.
    #[hegel::test]
    fn the_advised_wait_is_the_shortest_one_that_works(tc: hegel::TestCase) {
        let mut limiter = RateLimiter::new();
        let mut clock: Millis = 0;
        for (kind, requested_at) in any_schedule(&tc) {
            let requested_at = requested_at.max(clock);
            let wait = limiter.check(kind, requested_at).wait_millis();
            if wait > 0 {
                assert_ne!(
                    limiter.check(kind, requested_at + wait - 1),
                    Decision::Proceed,
                    "the limiter waited longer than it had to"
                );
            }
            clock = requested_at + wait;
            limiter.record(kind, clock);
        }
    }
}
