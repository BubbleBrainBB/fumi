//! Where a dragged sidebar settles and the widths it snaps to. Port of
//! `Views/Sidebar/SidebarWidthPolicy.swift`. Widths are points.

pub const MINIMUM: f64 = 200.0;
pub const STANDARD_DEFAULT: f64 = 240.0;
pub const STANDARD_MAX: f64 = 280.0;
pub const WIDE_MIN: f64 = 300.0;
pub const WIDE_SNAP: f64 = 320.0;
pub const MAX: f64 = 420.0;

/// Released this close to a snap point, the width lands exactly on it.
pub const SNAP_DISTANCE: f64 = 8.0;
/// Widths that give a haptic tick when the divider passes them.
pub const SNAP_POINTS: [f64; 2] = [STANDARD_DEFAULT, WIDE_SNAP];

// `Swift.min`/`Swift.max`, which keep NaN where `f64::min`/`max` drop it.
fn swift_min(x: f64, y: f64) -> f64 {
    if y < x { y } else { x }
}

fn swift_max(x: f64, y: f64) -> f64 {
    if y >= x { y } else { x }
}

/// The width the sidebar animates to when the divider is released at `width`.
/// Collapsing past the minimum is the split view's job.
pub fn settle(width: f64) -> f64 {
    if width < MINIMUM {
        MINIMUM
    } else if width <= STANDARD_MAX {
        if (width - STANDARD_DEFAULT).abs() <= SNAP_DISTANCE {
            STANDARD_DEFAULT
        } else {
            width.round()
        }
    } else if width < WIDE_MIN {
        // The 280-300 gap is a transition, not a resting place.
        if width - STANDARD_MAX < WIDE_MIN - width {
            STANDARD_MAX
        } else {
            WIDE_MIN
        }
    } else {
        let clamped = swift_min(width, MAX);
        if (clamped - WIDE_SNAP).abs() <= SNAP_DISTANCE {
            WIDE_SNAP
        } else {
            clamped.round()
        }
    }
}

/// The snap point crossed when the width moves from `old` to `new`, if any.
pub fn crossed_snap_point(old: f64, new: f64) -> Option<f64> {
    SNAP_POINTS
        .into_iter()
        .find(|&point| (old < point && new >= point) || (old > point && new <= point))
}

/// Double-click on the divider: fit the longest row, or go back to the default
/// when already fitted. `content_width` is the widest row's natural width.
pub fn double_click_target(current: f64, content_width: f64) -> f64 {
    let fit = swift_min(swift_max(content_width.ceil(), MINIMUM), MAX);
    if (current - fit).abs() < 1.0 { STANDARD_DEFAULT } else { fit }
}

#[cfg(test)]
mod tests {
    // Ported from Tests/SidebarWidthPolicyTests.swift.
    use super::*;

    #[test]
    fn settling_on_release() {
        assert_eq!(settle(170.0), 200.0, "under the minimum springs to 200");
        assert_eq!(settle(234.0), 240.0, "near 240 snaps to it");
        assert_eq!(settle(226.0), 226.0, "standard tier rests anywhere else");
        assert_eq!(settle(285.0), 280.0, "transition: closer to 280");
        assert_eq!(settle(295.0), 300.0, "transition: closer to 300");
        assert_eq!(settle(325.0), 320.0, "near 320 snaps to it");
        assert_eq!(settle(360.0), 360.0, "wide tier rests anywhere else");
        assert_eq!(settle(500.0), 420.0, "never wider than the max");
    }

    #[test]
    fn haptic_snap_points() {
        assert_eq!(crossed_snap_point(230.0, 245.0), Some(240.0), "crossing 240 rightwards");
        assert_eq!(crossed_snap_point(330.0, 318.0), Some(320.0), "crossing 320 leftwards");
        assert_eq!(crossed_snap_point(250.0, 260.0), None, "no snap point in between");
    }

    #[test]
    fn double_click() {
        assert_eq!(double_click_target(240.0, 351.2), 352.0, "fits the longest row");
        assert_eq!(double_click_target(352.0, 351.2), 240.0, "second double-click resets");
        assert_eq!(double_click_target(240.0, 900.0), 420.0, "fit is capped at the max");
        assert_eq!(double_click_target(260.0, 120.0), 200.0, "fit never goes below the standard min");
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;

    #[test]
    fn half_points_round_away_from_zero_like_swift() {
        assert_eq!(settle(226.5), 227.0);
        assert_eq!(settle(360.5), 361.0);
    }

    #[test]
    fn tier_edges() {
        assert_eq!(settle(200.0), 200.0);
        assert_eq!(settle(232.0), 240.0);
        assert_eq!(settle(248.0), 240.0);
        assert_eq!(settle(280.0), 280.0);
        assert_eq!(settle(290.0), 300.0, "the exact middle goes up");
        assert_eq!(settle(300.0), 300.0);
        assert_eq!(settle(420.0), 420.0);
    }

    #[test]
    fn nan_stays_nan_as_in_swift() {
        assert!(settle(f64::NAN).is_nan());
        assert!(double_click_target(240.0, f64::NAN).is_nan());
    }
}
