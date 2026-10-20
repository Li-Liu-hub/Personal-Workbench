//! Native drag sessions do not deliver ordinary pointer-move events on Windows.
//! Read the live client position instead of reusing egui's pre-drag pointer.
use eframe::egui;

pub fn client_position(window: usize, pixels_per_point: f32) -> Option<egui::Pos2> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::{HWND, POINT},
            Graphics::Gdi::ScreenToClient,
            UI::WindowsAndMessaging::{GetCursorPos, IsWindow},
        };
        let window = window as HWND;
        let mut point = POINT::default();
        // The HWND belongs to the current eframe window. These APIs only read
        // its coordinate system and the cursor; they never change input/focus.
        if unsafe { IsWindow(window) } == 0
            || unsafe { GetCursorPos(&mut point) } == 0
            || unsafe { ScreenToClient(window, &mut point) } == 0
        {
            return None;
        }
        logical_position(point.x, point.y, pixels_per_point)
    }
    #[cfg(not(windows))]
    {
        let _ = (window, pixels_per_point);
        None
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn logical_position(x: i32, y: i32, scale: f32) -> Option<egui::Pos2> {
    (scale.is_finite() && scale > 0.0).then(|| egui::pos2(x as f32 / scale, y as f32 / scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_pixels_follow_display_scale_including_positions_outside_the_window() {
        assert_eq!(
            logical_position(1050, 675, 1.5),
            Some(egui::pos2(700.0, 450.0))
        );
        assert_eq!(
            logical_position(-30, 80, 2.0),
            Some(egui::pos2(-15.0, 40.0))
        );
        assert_eq!(logical_position(0, 0, 0.0), None);
        assert_eq!(logical_position(0, 0, f32::NAN), None);
        assert_eq!(client_position(0, 1.0), None);
    }
}
