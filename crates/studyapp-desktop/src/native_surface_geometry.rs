//! Pure geometry and OS-drag gates shared by the native host and its tests.
use std::path::PathBuf;

pub type LogicalRect = (f64, f64, f64, f64);
pub type PhysicalRect = (i32, i32, i32, i32);

pub fn clipped_occlusions(
    editor: LogicalRect,
    dpi: u32,
    cards: &[LogicalRect],
) -> Vec<PhysicalRect> {
    let (x, y, width, height) = editor;
    let scale = f64::from(dpi) / 96.;
    let physical_x = (x * scale).round();
    let physical_y = (y * scale).round();
    let mut result = cards
        .iter()
        .filter_map(|&(left, top, w, h)| {
            if ![left, top, w, h].iter().all(|v| v.is_finite()) || w <= 0. || h <= 0. {
                return None;
            }
            let right = (left + w - x).min(width);
            let bottom = (top + h - y).min(height);
            let left = (left - x).max(0.);
            let top = (top - y).max(0.);
            (right > left && bottom > top).then_some((
                (((left + x) * scale).floor() - physical_x).max(0.) as i32,
                (((top + y) * scale).floor() - physical_y).max(0.) as i32,
                (((right + x) * scale).ceil() - physical_x).min((width * scale).round()) as i32,
                (((bottom + y) * scale).ceil() - physical_y).min((height * scale).round()) as i32,
            ))
        })
        .collect::<Vec<PhysicalRect>>();
    result.sort_unstable();
    result.dedup();
    result
}

#[derive(Default)]
pub struct DragGate {
    paths: Option<Vec<PathBuf>>,
}
impl DragGate {
    pub fn enter(&mut self, paths: Vec<PathBuf>) -> bool {
        if paths.is_empty() || self.paths.as_ref() == Some(&paths) {
            return false;
        }
        self.paths = Some(paths);
        true
    }
    pub fn leave(&mut self) -> bool {
        self.paths.take().is_some()
    }
    pub fn drop_paths(&mut self, paths: Vec<PathBuf>) -> bool {
        self.paths.take().is_some() && !paths.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn occlusions_translate_clip_and_round_outward_at_fractional_dpi() {
        assert_eq!(
            clipped_occlusions(
                (100., 60., 400., 300.),
                144,
                &[(90., 50., 40.5, 40.5), (600., 60., 10., 10.)]
            ),
            vec![(0, 0, 46, 46)]
        );
    }
    #[test]
    fn no_intersection_restores_the_whole_native_window() {
        assert!(
            clipped_occlusions(
                (100., 60., 400., 300.),
                96,
                &[(0., 0., 20., 20.), (100., 20., 400., 40.)]
            )
            .is_empty()
        );
    }
    #[test]
    fn fractional_child_origin_matches_win32_rounded_position() {
        assert_eq!(
            clipped_occlusions((100.4, 60.4, 400., 300.), 144, &[(101.2, 61.2, 20., 20.)]),
            vec![(0, 0, 31, 31)]
        );
    }
    #[test]
    fn occlusion_order_and_duplicates_do_not_trigger_native_changes() {
        let a = (100., 60., 20., 20.);
        let b = (150., 100., 20., 20.);
        assert_eq!(
            clipped_occlusions((100., 60., 400., 300.), 96, &[a, b, a]),
            clipped_occlusions((100., 60., 400., 300.), 96, &[b, a])
        );
    }
    #[test]
    fn drag_coalesces_hover_but_allows_distinct_repeated_drops() {
        let mut gate = DragGate::default();
        let paths = vec![PathBuf::from("C:/notes/a.md")];
        assert!(!gate.enter(Vec::new()));
        assert!(gate.enter(paths.clone()));
        assert!(!gate.enter(paths.clone()));
        assert!(gate.drop_paths(paths.clone()));
        assert!(!gate.drop_paths(paths.clone()));
        assert!(!gate.leave());
        assert!(gate.enter(paths.clone()));
        assert!(gate.drop_paths(paths));
    }
    #[test]
    fn drag_leave_emits_once_and_empty_drop_does_not_import() {
        let mut gate = DragGate::default();
        assert!(gate.enter(vec![PathBuf::from("C:/notes")]));
        assert!(gate.leave());
        assert!(!gate.leave());
        assert!(!gate.drop_paths(Vec::new()));
    }
}
