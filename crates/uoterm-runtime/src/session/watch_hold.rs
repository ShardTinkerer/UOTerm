//! The window asks for the whole screen with the `watch` tool many times a
//! second, and the session builds that picture on the same loop that sends
//! the steps. A step that falls due while a picture is being built waits
//! for it, and the step after it then leaves at once to keep the rhythm of
//! the walk: the character stops and jumps. A debug build took 19 to 35 ms
//! for one picture of a busy town, and the steps left in pairs.
//!
//! So a call for a picture waits while a step falls due before the picture
//! could be done. It is answered as soon as the step is out. The window gets
//! its picture a few milliseconds later; the step leaves at its slot.

use super::*;

/// How much longer than the slowest recent picture a step must be away for
/// a picture to start before it. One picture takes a little longer or
/// shorter than the one before it.
const COST_MARGIN: f64 = 1.5;
/// How much of the slowest recent time each new picture keeps. A slow
/// picture sets the time at once; the time comes down slowly after it.
const COST_KEPT: f64 = 0.9;

/// The calls for a picture that wait for a step, and how long a picture
/// takes to build.
#[derive(Debug, Default)]
pub(super) struct WatchHold {
    held: Vec<(ToolCall, oneshot::Sender<ToolResult>)>,
    /// The slowest recent picture, coming down slowly.
    cost: Duration,
}

impl WatchHold {
    /// True when a picture started now could still be building when the
    /// step due at `step_due` falls due.
    pub(super) fn step_comes_first(&self, step_due: Option<Instant>, now: Instant) -> bool {
        let room = self.cost.mul_f64(COST_MARGIN);
        step_due.is_some_and(|due| due.saturating_duration_since(now) <= room)
    }

    pub(super) fn hold(&mut self, call: ToolCall, reply: oneshot::Sender<ToolResult>) {
        self.held.push((call, reply));
    }

    /// The calls that waited, to answer now.
    pub(super) fn release(&mut self) -> Vec<(ToolCall, oneshot::Sender<ToolResult>)> {
        std::mem::take(&mut self.held)
    }

    /// Learns how long one picture took to build.
    pub(super) fn note_cost(&mut self, took: Duration) {
        self.cost = took.max(self.cost.mul_f64(COST_KEPT));
    }
}

/// Builds the picture a call asked for and sends it back. A step that is
/// already due goes out first, and the time the picture took is learned.
pub(super) fn answer_watch(
    inner: &mut Inner,
    hold: &mut WatchHold,
    call: ToolCall,
    reply: oneshot::Sender<ToolResult>,
) {
    pump_movement(inner, Instant::now());
    let started = Instant::now();
    let answer = answer_now(inner, call);
    hold.note_cost(started.elapsed());
    let _ = reply.send(answer);
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP_AWAY: Duration = Duration::from_millis(30);
    const PICTURE: Duration = Duration::from_millis(25);

    fn watch_call() -> ToolCall {
        ToolCall {
            name: TOOL_WATCH.into(),
            args: json!({}),
        }
    }

    #[test]
    fn a_picture_starts_when_no_step_is_due() {
        let mut hold = WatchHold::default();
        hold.note_cost(PICTURE);
        assert!(!hold.step_comes_first(None, Instant::now()));
    }

    #[test]
    fn a_step_due_before_the_picture_could_be_done_goes_first() {
        let mut hold = WatchHold::default();
        hold.note_cost(PICTURE);
        let now = Instant::now();
        assert!(hold.step_comes_first(Some(now + STEP_AWAY), now));
        assert!(!hold.step_comes_first(Some(now + STEP_AWAY * 4), now));
    }

    #[test]
    fn a_fast_picture_never_waits_for_a_step_further_away_than_it_takes() {
        let mut hold = WatchHold::default();
        hold.note_cost(Duration::from_millis(2));
        let now = Instant::now();
        assert!(!hold.step_comes_first(Some(now + STEP_AWAY), now));
    }

    #[test]
    fn a_slow_picture_sets_the_cost_at_once_and_it_comes_down_slowly() {
        let mut hold = WatchHold::default();
        hold.note_cost(PICTURE * 2);
        hold.note_cost(Duration::ZERO);
        assert_eq!(hold.cost, (PICTURE * 2).mul_f64(COST_KEPT));
        hold.note_cost(PICTURE * 3);
        assert_eq!(hold.cost, PICTURE * 3);
    }

    #[test]
    fn the_held_calls_are_released_once() {
        let mut hold = WatchHold::default();
        let (reply, _answer) = oneshot::channel();
        hold.hold(watch_call(), reply);
        assert_eq!(hold.release().len(), 1);
        assert!(hold.release().is_empty());
    }
}
