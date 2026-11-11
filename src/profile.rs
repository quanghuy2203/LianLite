//! Local profile presets. Preview-only: never writes to any device.
use crate::{Config, Group};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Preset { Silent, Balanced, Performance }
impl Preset {
    pub fn name(self) -> &'static str {
        match self { Self::Silent => "Silent", Self::Balanced => "Balanced", Self::Performance => "Performance" }
    }
    pub fn speed(self) -> u8 {
        match self { Self::Silent => 35, Self::Balanced => 65, Self::Performance => 85 }
    }
}
pub fn apply(config: &mut Config, preset: Preset) {
    for g in &mut config.groups { g.speed_percent = preset.speed(); }
}
pub fn all_same_color(config: &mut Config, rgb: [u8; 3]) {
    for g in &mut config.groups { g.rgb = rgb; }
}
pub fn all_brightness(config: &mut Config, value: u8) {
    let value = value.min(100);
    for g in &mut config.groups { g.brightness = value; }
    config.strimer_brightness = [value, value];
}
pub fn validate(config: &mut Config) {
    // Preserve user layout, but prevent accidental invalid preview values.
    for Group { speed_percent, brightness, .. } in &mut config.groups {
        *speed_percent = (*speed_percent).clamp(20, 100);
        *brightness = (*brightness).min(100);
    }
    for value in &mut config.strimer_brightness { *value = (*value).min(100); }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn presets_only_change_speed() {
        let mut cfg = Config::default();
        let color = cfg.groups[0].rgb;
        apply(&mut cfg, Preset::Silent);
        assert!(cfg.groups.iter().all(|g| g.speed_percent == 35 && g.rgb == color));
        apply(&mut cfg, Preset::Performance);
        assert!(cfg.groups.iter().all(|g| g.speed_percent == 85));
    }
    #[test] fn sync_preview_clamps_brightness() {
        let mut cfg = Config::default();
        all_same_color(&mut cfg, [9,8,7]);
        all_brightness(&mut cfg, 120);
        assert!(cfg.groups.iter().all(|g| g.rgb == [9,8,7] && g.brightness == 100));
        assert_eq!(cfg.strimer_brightness, [100,100]);
    }
}
