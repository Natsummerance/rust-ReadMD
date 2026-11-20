//! Logical top-left geometry only; AppKit screen enumeration stays on its GUI
//! thread. A monitor change must not strand the first WebView paint offscreen.
use crate::protocol::SnapshotBounds;

#[derive(Debug, Clone, Copy)]
pub(super) struct ScreenArea {
    pub frame: SnapshotBounds,
    pub visible: SnapshotBounds,
}

fn valid(rect: SnapshotBounds) -> bool {
    [rect.x, rect.y, rect.width, rect.height]
        .iter()
        .all(|value| value.is_finite())
        && rect.width > 0.0
        && rect.height > 0.0
}

fn intersects(a: SnapshotBounds, b: SnapshotBounds) -> bool {
    a.x < b.x + b.width
        && b.x < a.x + a.width
        && a.y < b.y + b.height
        && b.y < a.y + a.height
}

/// Preserve every partly visible placement, including negative-coordinate
/// monitors. Only a wholly disconnected rectangle is moved, never resized.
pub(super) fn recover_offscreen(
    bounds: SnapshotBounds,
    screens: &[ScreenArea],
    dragging: bool,
) -> SnapshotBounds {
    if dragging
        || !valid(bounds)
        || screens
            .iter()
            .any(|screen| valid(screen.frame) && intersects(bounds, screen.frame))
    {
        return bounds;
    }
    screens
        .iter()
        .filter(|screen| valid(screen.frame))
        .map(|screen| {
            let area = if valid(screen.visible) && intersects(screen.visible, screen.frame) {
                screen.visible
            } else {
                screen.frame
            };
            let placed = SnapshotBounds {
                x: bounds.x.clamp(area.x, (area.x + area.width - bounds.width).max(area.x)),
                y: bounds.y.clamp(area.y, (area.y + area.height - bounds.height).max(area.y)),
                ..bounds
            };
            let distance = (placed.x - bounds.x).powi(2) + (placed.y - bounds.y).powi(2);
            (placed, distance)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(bounds, |(placed, _)| placed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> SnapshotBounds {
        SnapshotBounds { x, y, width, height }
    }
    fn screen(x: f64, y: f64, width: f64, height: f64) -> ScreenArea {
        let frame = rect(x, y, width, height);
        ScreenArea { frame, visible: frame }
    }
    fn pet(x: f64, y: f64) -> SnapshotBounds {
        SnapshotBounds { x, y, ..SnapshotBounds::default() }
    }

    #[test]
    fn old_landscape_position_is_recovered_on_the_actual_portrait_screen() {
        let original = pet(1639.0, 309.0);
        assert_eq!(recover_offscreen(original, &[screen(0.0, 0.0, 1200.0, 1920.0)], false), pet(880.0, 309.0));
    }

    #[test]
    fn negative_and_above_primary_monitor_positions_remain_legal() {
        let screens = [screen(0.0, 0.0, 1200.0, 1920.0), screen(-1600.0, 100.0, 1600.0, 900.0), screen(0.0, -1080.0, 1920.0, 1080.0)];
        for original in [pet(-1500.0, 200.0), pet(100.0, -1000.0)] {
            assert_eq!(recover_offscreen(original, &screens, false), original);
        }
        // Recovery follows the nearest real display, rather than the primary.
        assert_eq!(recover_offscreen(pet(-2100.0, 200.0), &screens, false), pet(-1600.0, 200.0));
    }

    #[test]
    fn a_single_visible_pixel_on_any_edge_preserves_the_requested_position() {
        let screens = [screen(0.0, 0.0, 1200.0, 1920.0)];
        for original in [pet(-319.0, 300.0), pet(1199.0, 300.0), pet(100.0, -419.0), pet(100.0, 1919.0)] {
            assert_eq!(recover_offscreen(original, &screens, false), original);
        }
        assert_eq!(recover_offscreen(pet(1200.0, 300.0), &screens, false), pet(880.0, 300.0));
    }

    #[test]
    fn screen_gaps_are_not_mistaken_for_the_union_bounding_box() {
        let screens = [screen(0.0, 0.0, 1000.0, 1000.0), screen(2000.0, 0.0, 1000.0, 1000.0)];
        assert_eq!(recover_offscreen(pet(1200.0, 200.0), &screens, false), pet(680.0, 200.0));
    }

    #[test]
    fn recovery_uses_the_work_area_without_resizing_or_interfering_with_dragging() {
        let screens = [ScreenArea {
            frame: rect(0.0, 0.0, 1200.0, 1920.0),
            visible: rect(0.0, 24.0, 1150.0, 1840.0),
        }];
        let original = pet(1639.0, 1900.0);
        assert_eq!(recover_offscreen(original, &screens, true), original);
        assert_eq!(recover_offscreen(original, &screens, false), pet(830.0, 1444.0));
        let large = rect(2000.0, 2000.0, 600.0, 800.0);
        assert_eq!(recover_offscreen(large, &[screen(0.0, 0.0, 320.0, 300.0)], false), rect(0.0, 0.0, 600.0, 800.0));
    }

    #[test]
    fn removing_and_reconnecting_a_display_does_not_restore_an_offscreen_position() {
        let portrait = screen(0.0, 0.0, 1200.0, 1920.0);
        let right = screen(1200.0, 0.0, 1920.0, 1080.0);
        let original = pet(1639.0, 309.0);
        assert_eq!(recover_offscreen(original, &[portrait, right], false), original);
        let recovered = recover_offscreen(original, &[portrait], false);
        assert_eq!(recovered, pet(880.0, 309.0));
        assert_eq!(recover_offscreen(recovered, &[portrait, right], false), recovered);
        assert_eq!(recover_offscreen(original, &[], false), original);
        assert_eq!(recover_offscreen(original, &[screen(0.0, 0.0, 0.0, 0.0)], false), original);
    }
}
