//! Value transitions read at draw time, so a view can fade something out after
//! its state has already flipped. SwiftUI's `.easeOut` curve.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Instant;

/// `cubic-bezier(0, 0, 0.58, 1)`, what SwiftUI's `.easeOut` is.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    let x = |s: f32| 3. * (1. - s) * s * s * 0.58 + s * s * s;
    let y = |s: f32| 3. * (1. - s) * s * s + s * s * s;
    let (mut lo, mut hi) = (0f32, 1f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.;
        if x(mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    y((lo + hi) / 2.)
}

/// A value going to 1 when shown and to 0 when not.
#[derive(Clone, Copy, Debug)]
pub struct Transition {
    target: bool,
    from: f32,
    started: Option<Instant>,
    secs: f32,
}

impl Transition {
    pub fn new(shown: bool, secs: f32) -> Self {
        Transition { target: shown, from: if shown { 1. } else { 0. }, started: None, secs }
    }

    pub fn target(&self) -> bool {
        self.target
    }

    /// Heads for `shown` from wherever it is now.
    pub fn set(&mut self, shown: bool, now: Instant) {
        if shown != self.target {
            self.from = self.value(now);
            self.target = shown;
            self.started = Some(now);
        }
    }

    /// Jumps there.
    pub fn snap(&mut self, shown: bool) {
        *self = Transition::new(shown, self.secs);
    }

    fn end(&self) -> f32 {
        if self.target { 1. } else { 0. }
    }

    pub fn value(&self, now: Instant) -> f32 {
        let Some(started) = self.started else { return self.end() };
        let t = now.saturating_duration_since(started).as_secs_f32() / self.secs;
        if t >= 1. { self.end() } else { self.from + (self.end() - self.from) * ease_out(t) }
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.started.is_some_and(|s| now.saturating_duration_since(s).as_secs_f32() < self.secs)
    }

    /// Hidden and done fading: nothing to draw.
    pub fn is_gone(&self, now: Instant) -> bool {
        !self.target && !self.is_animating(now)
    }
}

/// A transition per key: hover fills, switches, folded rows.
pub struct Fades<K> {
    map: HashMap<K, Transition>,
    secs: f32,
}

impl<K: Eq + Hash + Clone> Fades<K> {
    pub fn new(secs: f32) -> Self {
        Fades { map: HashMap::new(), secs }
    }

    pub fn set(&mut self, key: K, on: bool, now: Instant) {
        let secs = self.secs;
        self.map.entry(key).or_insert_with(|| Transition::new(false, secs)).set(on, now);
    }

    pub fn snap(&mut self, key: K, on: bool) {
        self.map.insert(key, Transition::new(on, self.secs));
    }

    pub fn value(&self, key: &K, now: Instant) -> f32 {
        self.map.get(key).map_or(0., |t| t.value(now))
    }

    pub fn is_on(&self, key: &K) -> bool {
        self.map.get(key).is_some_and(Transition::target)
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.map.values().any(|t| t.is_animating(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn ease_out_starts_fast_and_ends_at_one() {
        assert!(ease_out(0.) < 1e-3);
        assert!((ease_out(1.) - 1.).abs() < 1e-3);
        assert!(ease_out(0.5) > 0.6);
        assert!(ease_out(0.25) < ease_out(0.5));
    }

    #[test]
    fn transition_runs_both_ways_and_ends() {
        let t0 = Instant::now();
        let mut t = Transition::new(false, 0.16);
        assert!(t.is_gone(t0));
        t.set(true, t0);
        assert!(!t.is_gone(t0));
        assert!(t.is_animating(t0 + Duration::from_millis(80)));
        let mid = t.value(t0 + Duration::from_millis(80));
        assert!(mid > 0. && mid < 1.);
        assert_eq!(t.value(t0 + Duration::from_millis(200)), 1.);
        // Reversed halfway it starts from where it was.
        let at = t0 + Duration::from_millis(80);
        t.set(false, at);
        assert!((t.value(at) - mid).abs() < 1e-5);
        assert!(!t.is_gone(at));
        assert!(t.is_gone(at + Duration::from_millis(170)));
        assert_eq!(t.value(at + Duration::from_millis(170)), 0.);
    }

    #[test]
    fn setting_the_same_target_changes_nothing() {
        let t0 = Instant::now();
        let mut t = Transition::new(true, 0.2);
        t.set(true, t0);
        assert!(!t.is_animating(t0));
        assert_eq!(t.value(t0), 1.);
    }

    #[test]
    fn fades_default_to_off() {
        let t0 = Instant::now();
        let mut f = Fades::new(0.12);
        assert_eq!(f.value(&"a", t0), 0.);
        f.set("a", true, t0);
        assert!(f.is_animating(t0));
        assert_eq!(f.value(&"a", t0 + Duration::from_secs(1)), 1.);
        f.snap("b", true);
        assert_eq!(f.value(&"b", t0), 1.);
        assert!(f.is_on(&"b"));
    }
}
