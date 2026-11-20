//! The rate of the frame callbacks of each window (F048).
//!
//! A Wayland client draws a new frame only after the frame callback of its last frame.
//! The compositor thus sets the frame rate of each app:
//!
//! - After input or a change of the window, the window gets the full rate for [`ACTIVE`].
//! - A window with video, or with a popup that has video, keeps the full rate.
//! - Any other window gets the idle rate: an animation with no user costs less host CPU.
//! - A window with no surface channel gets no frame callbacks: no user sees it.
//!
//! A popup or a subsurface follows its window: the input, the channels, and the video of
//! the window decide for it too.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The time of the full rate after input or a change of the window.
pub const ACTIVE: Duration = Duration::from_secs(2);

/// The idle rate if `FARGLASS_IDLE_FPS` does not give one: 10 frames each second.
/// The video detector needs 12 of 16 commits in 1.6 s, thus a video that starts with no input becomes video at this rate.
pub const IDLE_FPS: u32 = 10;

/// The pace state of one surface.
#[derive(Debug, Clone, Copy)]
struct Surface {
    /// The end of the full rate after input.
    active_until: Option<Instant>,
    /// The time of the last frame callbacks.
    last_sent: Option<Instant>,
    /// At least one surface channel shows the surface. A new surface counts as watched until the session tells otherwise.
    watched: bool,
    /// A surface channel sends a video region of the surface.
    video: bool,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            active_until: None,
            last_sent: None,
            watched: true,
            video: false,
        }
    }
}

/// The decision for the frame callbacks of a surface in one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Send the callbacks now.
    Send,
    /// Keep the callbacks for a later tick.
    Hold,
}

/// The rate of the frame callbacks of each surface.
#[derive(Debug)]
pub struct Pace {
    /// The interval of the idle rate, or `None` for the full rate at all times.
    idle: Option<Duration>,
    /// The interval of the timer that sends the callbacks.
    tick: Duration,
    surfaces: HashMap<u32, Surface>,
}

impl Pace {
    /// Returns a pace with the idle rate `idle_fps` and the timer interval `tick`. An `idle_fps` of 0 turns the idle rate off.
    #[must_use]
    pub fn new(idle_fps: u32, tick: Duration) -> Self {
        Self {
            idle: (idle_fps > 0).then(|| Duration::from_secs(1) / idle_fps),
            tick,
            surfaces: HashMap::new(),
        }
    }

    /// Returns the idle rate from `FARGLASS_IDLE_FPS`, or [`IDLE_FPS`].
    #[must_use]
    pub fn idle_fps_from_env() -> u32 {
        std::env::var("FARGLASS_IDLE_FPS")
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(IDLE_FPS)
    }

    /// Gives the full rate to the window `root` until [`ACTIVE`] after `now`.
    pub fn touch(&mut self, root: u32, now: Instant) {
        self.surfaces.entry(root).or_default().active_until = Some(now + ACTIVE);
    }

    /// Sets the channels and the video of the surface `id`.
    pub fn set_viewers(&mut self, id: u32, watched: bool, video: bool) {
        let surface = self.surfaces.entry(id).or_default();
        surface.watched = watched;
        surface.video = video;
    }

    /// Forgets the surface `id`, after it closed.
    pub fn remove(&mut self, id: u32) {
        self.surfaces.remove(&id);
    }

    /// Reports whether the window `root` is watched.
    #[must_use]
    pub fn watched(&self, root: u32) -> bool {
        self.surfaces.get(&root).is_none_or(|s| s.watched)
    }

    /// Decides for the callbacks of the surface `id` in the window `root` at `now`.
    /// `Send` records the time: call it one time for each surface and tick.
    pub fn decide(&mut self, id: u32, root: u32, now: Instant) -> Decision {
        let window = self.surfaces.get(&root).copied().unwrap_or_default();
        if !window.watched {
            return Decision::Hold;
        }
        let video = window.video || self.surfaces.get(&id).is_some_and(|s| s.video);
        let active = window.active_until.is_some_and(|until| now < until);
        let surface = self.surfaces.entry(id).or_default();
        let due = match self.idle {
            None => true,
            Some(_) if video || active => true,
            // The timer ticks at its own interval: a callback within half a tick of the interval goes now, not one tick later.
            Some(idle) => surface
                .last_sent
                .is_none_or(|last| now.saturating_duration_since(last) + self.tick / 2 >= idle),
        };
        if due {
            surface.last_sent = Some(now);
            Decision::Send
        } else {
            Decision::Hold
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: Duration = Duration::from_millis(16);

    /// Returns the count of sends of the surface `id` in `root` in each tick from `start` for `time`.
    fn sends(pace: &mut Pace, id: u32, root: u32, start: Instant, time: Duration) -> usize {
        let ticks = time.as_millis() / TICK.as_millis();
        (0..ticks)
            .filter(|&i| {
                let now = start + TICK * u32::try_from(i).unwrap();
                pace.decide(id, root, now) == Decision::Send
            })
            .count()
    }

    #[test]
    fn an_idle_window_gets_the_idle_rate_and_input_gives_the_full_rate() {
        let mut pace = Pace::new(10, TICK);
        let start = Instant::now();
        let idle = sends(&mut pace, 1, 1, start, Duration::from_secs(1));
        assert!(
            (9..=11).contains(&idle),
            "{idle} frames in 1 s with no input"
        );
        let later = start + Duration::from_secs(1);
        pace.touch(1, later);
        let active = sends(&mut pace, 1, 1, later, Duration::from_secs(1));
        assert_eq!(active, 62, "each tick for 1 s after input");
        // 2 s after the input, the rate goes down again.
        let after = later + ACTIVE;
        let idle = sends(&mut pace, 1, 1, after, Duration::from_secs(1));
        assert!(
            (9..=11).contains(&idle),
            "{idle} frames in 1 s after the active time"
        );
    }

    #[test]
    fn the_idle_rate_is_fast_enough_for_the_video_detector() {
        let mut pace = Pace::new(IDLE_FPS, TICK);
        let start = Instant::now();
        let mut times = Vec::new();
        for i in 0..200u32 {
            let now = start + TICK * i;
            if pace.decide(1, 1, now) == Decision::Send {
                times.push(now);
            }
        }
        for window in times.windows(crate::video::WINDOW) {
            assert!(window[crate::video::WINDOW - 1] - window[0] <= crate::video::WINDOW_TIME);
        }
    }

    #[test]
    fn a_window_with_no_channel_gets_no_callbacks_and_its_popup_neither() {
        let mut pace = Pace::new(10, TICK);
        let now = Instant::now();
        pace.set_viewers(1, false, false);
        pace.touch(1, now);
        assert_eq!(pace.decide(1, 1, now), Decision::Hold);
        assert_eq!(
            pace.decide(2, 1, now),
            Decision::Hold,
            "a popup of the window"
        );
        pace.set_viewers(1, true, false);
        assert_eq!(pace.decide(1, 1, now), Decision::Send);
    }

    #[test]
    fn video_keeps_the_full_rate() {
        let mut pace = Pace::new(10, TICK);
        let start = Instant::now();
        pace.set_viewers(1, true, true);
        assert_eq!(sends(&mut pace, 1, 1, start, Duration::from_secs(1)), 62);
        // A subsurface with video keeps its rate, and its window follows the idle rate.
        let mut pace = Pace::new(10, TICK);
        pace.set_viewers(2, true, true);
        assert_eq!(sends(&mut pace, 2, 1, start, Duration::from_secs(1)), 62);
        assert!(sends(&mut pace, 1, 1, start, Duration::from_secs(1)) <= 11);
    }

    #[test]
    fn an_idle_rate_of_zero_is_the_full_rate() {
        let mut pace = Pace::new(0, TICK);
        assert_eq!(
            sends(&mut pace, 1, 1, Instant::now(), Duration::from_secs(1)),
            62
        );
    }
}
