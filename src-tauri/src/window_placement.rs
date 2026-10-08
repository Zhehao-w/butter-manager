//! Place the startup window before showing it. Geometry uses physical pixels.
#[cfg(any(feature = "desktop", test))]
fn fit_inner(work: (u32, u32), desired: (u32, u32), frame: (u32, u32), margin: u32) -> (u32, u32) {
    let available = |total: u32, decoration: u32| {
        total
            .saturating_sub(margin.saturating_mul(2))
            .saturating_sub(decoration)
            .max(1)
    };
    (
        desired.0.min(available(work.0, frame.0)),
        desired.1.min(available(work.1, frame.1)),
    )
}

#[cfg(any(feature = "desktop", test))]
fn centered(origin: (i32, i32), work: (u32, u32), outer: (u32, u32)) -> (i32, i32) {
    let coordinate = |start: i32, total: u32, size: u32| {
        (i64::from(start) + i64::from(total.saturating_sub(size) / 2))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    };
    (
        coordinate(origin.0, work.0, outer.0),
        coordinate(origin.1, work.1, outer.1),
    )
}

#[cfg(feature = "desktop")]
pub fn position(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    use tauri::{PhysicalPosition, PhysicalSize};
    let monitor = window
        .cursor_position()
        .ok()
        .and_then(|cursor| window.monitor_from_point(cursor.x, cursor.y).ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())
        .or_else(|| window.current_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return window.center();
    };
    let work = monitor.work_area();
    let initial = window.inner_size()?;
    let initial_scale = window.scale_factor()?;
    let scale = monitor.scale_factor();
    let desired = (
        (f64::from(initial.width) / initial_scale * scale).round() as u32,
        (f64::from(initial.height) / initial_scale * scale).round() as u32,
    );
    // Move the hidden window onto the target monitor before measuring its DPI-scaled frame.
    window.set_position(work.position)?;
    let outer = window.outer_size()?;
    let inner = window.inner_size()?;
    let frame = (
        outer.width.saturating_sub(inner.width),
        outer.height.saturating_sub(inner.height),
    );
    let fitted = fit_inner(
        (work.size.width, work.size.height),
        desired,
        frame,
        (16.0 * scale).round() as u32,
    );
    window.set_min_size(Some(PhysicalSize::new(
        ((850.0 * scale).round() as u32).min(fitted.0),
        ((580.0 * scale).round() as u32).min(fitted.1),
    )))?;
    window.set_size(PhysicalSize::new(fitted.0, fitted.1))?;
    let outer = window.outer_size()?;
    let (x, y) = centered(
        (work.position.x, work.position.y),
        (work.size.width, work.size.height),
        (outer.width, outer.height),
    );
    window.set_position(PhysicalPosition::new(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn centers_inside_offset_work_area_including_negative_monitor_coordinates() {
        // Left monitor, with a taskbar at its top.
        assert_eq!(
            centered((-1920, 48), (1920, 1032), (1456, 940)),
            (-1688, 94)
        );
        // A monitor above the primary screen.
        assert_eq!(
            centered((240, -1440), (2560, 1392), (1600, 1000)),
            (720, -1244)
        );
    }
    #[test]
    fn shrinks_high_dpi_window_to_work_area_without_losing_its_frame_or_margin() {
        let inner = fit_inner((1920, 1040), (2160, 1350), (24, 60), 24);
        assert_eq!(inner, (1848, 932));
        assert_eq!(
            centered((0, 0), (1920, 1040), (inner.0 + 24, inner.1 + 60)),
            (24, 24)
        );
        assert_eq!(
            fit_inner((2560, 1400), (1440, 900), (16, 40), 16),
            (1440, 900)
        );
    }
}
