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
    fn config_with_group() -> Config {
        let mut cfg = Config::default();
        cfg.groups.push(Group {
            name: "Test preview zone".into(), count: 2,
            speed_percent: 65, brightness: 70, rgb: [100, 150, 255],
        });
        cfg
    }
    #[test] fn presets_only_change_speed() {
        let mut cfg = config_with_group();
        let original_color = cfg.groups[0].rgb;
        let original_brightness = cfg.groups[0].brightness;
        apply(&mut cfg, Preset::Silent);
        assert_eq!(cfg.groups[0].speed_percent, 35);
        assert_eq!(cfg.groups[0].rgb, original_color);
        assert_eq!(cfg.groups[0].brightness, original_brightness);
        apply(&mut cfg, Preset::Performance);
        assert_eq!(cfg.groups[0].speed_percent, 85);
    }
    #[test] fn sync_preview_clamps_brightness() {
        let mut cfg = config_with_group();
        all_same_color(&mut cfg, [9,8,7]);
        all_brightness(&mut cfg, 120);
        assert_eq!(cfg.groups[0].rgb, [9,8,7]);
        assert_eq!(cfg.groups[0].brightness, 100);
        assert_eq!(cfg.strimer_brightness, [100,100]);
    }
    #[test] fn empty_inventory_is_safe() {
        let mut cfg = Config::default();
        assert!(cfg.groups.is_empty(), "No fan groups should be invented");
        apply(&mut cfg, Preset::Silent);
        all_same_color(&mut cfg, [1,2,3]);
        all_brightness(&mut cfg, 85);
        validate(&mut cfg);
        assert!(cfg.groups.is_empty());
        assert_eq!(cfg.strimer_brightness, [85,85]);
    }
}
