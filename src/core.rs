use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub const MAX_TEMP: f64 = 85.0;
pub const FULL_SPEED_TEMP: f64 = 80.0;
pub const MIN_PERCENT: u8 = 40;
pub const STALE_AFTER: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Profile {
    Firmware,
    #[serde(alias = "Custom")]
    Auto,
    Quiet,
    Cool,
    Intensive,
    Maximum,
}
impl Profile {
    pub const ALL: [Self; 6] = [
        Self::Firmware,
        Self::Auto,
        Self::Quiet,
        Self::Cool,
        Self::Intensive,
        Self::Maximum,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Firmware => "Automático Acer",
            Self::Auto => "Auto",
            Self::Quiet => "Discreto",
            Self::Cool => "Resfriar",
            Self::Intensive => "Carga intensa",
            Self::Maximum => "Ventilação máxima",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Firmware => {
                "A Acer administra a ventoinha. O A-Fan apenas monitora; a curva do firmware não é conhecida."
            }
            Self::Auto => {
                "Uso diário. Aumenta a ventilação com a temperatura e reduz devagar após esfriar."
            }
            Self::Quiet => {
                "Menos rotação em carga leve. A ventilação sobe rapidamente quando a CPU aquece."
            }
            Self::Cool => "Mais ventilação desde temperaturas baixas. Atinge 100% aos 75°C.",
            Self::Intensive => "Prioriza refrigeração em tarefas pesadas. Atinge 100% aos 65°C.",
            Self::Maximum => "Mantém a ventoinha em 100% enquanto o controle estiver ativo.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub celsius: f64,
    pub percent: u8,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub points: Vec<Point>,
}
impl Curve {
    pub fn preset(profile: Profile, floor: u8) -> Self {
        let values: &[(f64, u8)] = match profile {
            Profile::Quiet => &[
                (40., 40),
                (55., 40),
                (65., 55),
                (75., 85),
                (80., 100),
                (85., 100),
            ],
            Profile::Cool => &[
                (40., 50),
                (55., 65),
                (65., 80),
                (75., 100),
                (80., 100),
                (85., 100),
            ],
            Profile::Intensive => &[
                (40., 60),
                (55., 80),
                (65., 100),
                (75., 100),
                (80., 100),
                (85., 100),
            ],
            Profile::Maximum => &[(20., 100), (85., 100)],
            _ => &[
                (40., 40),
                (55., 50),
                (65., 65),
                (75., 90),
                (80., 100),
                (85., 100),
            ],
        };
        Self {
            points: values
                .iter()
                .map(|&(celsius, percent)| Point {
                    celsius,
                    percent: percent.max(floor).max(MIN_PERCENT),
                })
                .collect(),
        }
    }
    pub fn validate(&self, floor: u8) -> Result<(), String> {
        if !(2..=8).contains(&self.points.len()) || !(MIN_PERCENT..=100).contains(&floor) {
            return Err("Preset ou mínimo de rotação inválido.".into());
        }
        for (index, p) in self.points.iter().enumerate() {
            if !p.celsius.is_finite()
                || !(20.0..=MAX_TEMP).contains(&p.celsius)
                || p.percent < floor
                || p.percent > 100
            {
                return Err(format!(
                    "Ponto {} fora dos limites: 20–85°C e {}–100%.",
                    index + 1,
                    floor
                ));
            }
            if index > 0
                && (p.celsius <= self.points[index - 1].celsius
                    || p.percent < self.points[index - 1].percent)
            {
                return Err("Temperaturas devem crescer e a velocidade não pode diminuir.".into());
            }
        }
        if self.points.last()
            != Some(&Point {
                celsius: MAX_TEMP,
                percent: 100,
            })
        {
            return Err("O último ponto deve ser 85°C → 100% (proteção térmica).".into());
        }
        if !self
            .points
            .iter()
            .any(|p| p.celsius <= FULL_SPEED_TEMP && p.percent == 100)
        {
            return Err("O preset deve atingir 100% até 80°C.".into());
        }
        Ok(())
    }
    pub fn percent(&self, temperature: f64) -> u8 {
        if !temperature.is_finite() || temperature <= 0. || temperature >= FULL_SPEED_TEMP {
            return 100;
        }
        for pair in self.points.windows(2) {
            let [a, b] = [pair[0], pair[1]];
            if temperature <= a.celsius {
                return a.percent;
            }
            if temperature <= b.celsius {
                return (a.percent as f64
                    + (b.percent - a.percent) as f64 * (temperature - a.celsius)
                        / (b.celsius - a.celsius))
                    .ceil() as u8;
            }
        }
        self.points.last().map_or(100, |p| p.percent)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Calibration {
    pub model: String,
    pub bios: String,
    pub minimum_percent: u8,
    pub version: u32,
    pub temperature_and_rpm_verified: bool,
    pub independent_speed_verified: bool,
    pub recovery_verified: bool,
}
impl Calibration {
    pub fn valid_for(&self, model: &str, bios: &str) -> bool {
        self.version == 2
            && self.model == model
            && self.bios == bios
            && (30..=100).contains(&self.minimum_percent)
            && self.temperature_and_rpm_verified
            && self.independent_speed_verified
            && self.recovery_verified
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    pub profile: Profile,
    pub calibration: Option<Calibration>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 2,
            profile: Profile::Auto,
            calibration: None,
        }
    }
}
impl Settings {
    pub fn floor(&self) -> u8 {
        self.calibration
            .as_ref()
            .map_or(MIN_PERCENT, |c| c.minimum_percent.max(MIN_PERCENT))
    }
    pub fn curve(&self) -> Curve {
        Curve::preset(self.profile, self.floor())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 2 {
            return Err("Formato de configurações não reconhecido.".into());
        }
        self.curve().validate(self.floor())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Telemetry {
    pub temperature: Option<f64>,
    pub rpm: Option<u32>,
    pub fan_percent: Option<u8>,
    pub fan_mode: Option<u8>,
    pub controls_available: bool,
    pub reason: String,
}
impl Telemetry {
    pub fn temperature_valid(&self) -> bool {
        self.temperature
            .is_some_and(|t| t.is_finite() && t > 0. && t <= 110.)
    }
}

pub struct Controller {
    current: Option<u8>,
    cooling_since: Option<Instant>,
    last_drop: Option<Instant>,
    overheated: bool,
}
impl Controller {
    pub fn new() -> Self {
        Self {
            current: None,
            cooling_since: None,
            last_drop: None,
            overheated: false,
        }
    }
    pub fn reset(&mut self) {
        let overheated = self.overheated;
        *self = Self::new();
        self.overheated = overheated;
    }
    pub fn commanded(&self) -> Option<u8> {
        self.current
    }
    pub fn next(&mut self, curve: &Curve, temperature: f64, now: Instant) -> u8 {
        if temperature >= MAX_TEMP {
            self.overheated = true;
        }
        if self.overheated {
            if temperature.is_finite() && temperature > 0. && temperature <= 75. {
                let since = *self.cooling_since.get_or_insert(now);
                if now.duration_since(since) >= Duration::from_secs(30) {
                    self.overheated = false;
                }
            } else {
                self.cooling_since = None;
            }
            if self.overheated {
                self.current = Some(100);
                return 100;
            }
        }
        let target = curve.percent(temperature);
        let current = self.current.unwrap_or(100);
        let output = if target >= current || temperature >= MAX_TEMP {
            self.cooling_since = None;
            target
        } else {
            let cooling = curve.percent((temperature + 3.).min(MAX_TEMP));
            if cooling >= current {
                self.cooling_since = None;
                current
            } else {
                let start = *self.cooling_since.get_or_insert(now);
                if now.duration_since(start) >= Duration::from_secs(30)
                    && self
                        .last_drop
                        .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1))
                {
                    self.last_drop = Some(now);
                    current.saturating_sub(5).max(cooling)
                } else {
                    current
                }
            }
        };
        self.current = Some(output);
        output
    }
}

pub fn status_ok(raw: u64) -> Result<u64, String> {
    match raw & 255 {
        0 => Ok(raw),
        1 => Err("O firmware não oferece este comando (status 1).".into()),
        code => Err(format!("O firmware recusou o comando (status {code}).")),
    }
}
pub fn decode_control(behavior: u64, speed: u64) -> Result<(u8, u8), String> {
    let mode = ((status_ok(behavior)? >> 8) & 3) as u8;
    let percent = ((status_ok(speed)? >> 8) & 255) as u8;
    if !(1..=3).contains(&mode) || percent > 100 {
        return Err("Modo ou velocidade de ventoinha inválido.".into());
    }
    Ok((mode, percent))
}
pub fn sensor_input(id: u8) -> u32 {
    (u32::from(id) << 8) | 1
}
pub fn sensor_value(raw: u64) -> Result<u16, String> {
    Ok(((status_ok(raw)? >> 8) & 0xffff) as u16)
}
pub fn fan_behavior_input(mode: u8) -> u64 {
    (u64::from(mode) << 16) | 1
}
pub fn fan_speed_input(percent: u8) -> Result<u64, String> {
    if !(1..=100).contains(&percent) {
        return Err("Velocidade fora dos limites.".into());
    }
    Ok((u64::from(percent) << 8) | 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn firmware_status_is_never_a_sensor_or_percent() {
        assert!(sensor_value(1).is_err());
        assert_eq!(sensor_value(0x4600).unwrap(), 70);
        assert_eq!(sensor_value(0xfcd00).unwrap(), 4045);
        assert_eq!(sensor_input(2), 0x201);
        assert_eq!(fan_behavior_input(3), 0x30001);
        assert_eq!(fan_speed_input(100).unwrap(), 0x6401);
        assert!(fan_speed_input(0).is_err());
        assert!(fan_speed_input(101).is_err());
        assert!(decode_control(1, 1).is_err());
        assert!(decode_control(0x300, 0x6500).is_err());
        assert_eq!(decode_control(0x300, 0x6400).unwrap(), (3, 100));
    }
    #[test]
    fn interpolation_and_immutable_thermal_limit() {
        let curve = Curve::preset(Profile::Auto, MIN_PERCENT);
        assert_eq!(curve.percent(20.), 40);
        assert_eq!(curve.percent(60.), 58);
        assert_eq!(curve.percent(90.), 100);
        assert!(curve.validate(MIN_PERCENT).is_ok());
        let mut bad = curve.clone();
        bad.points[1].percent = 20;
        assert!(bad.validate(MIN_PERCENT).is_err());
        bad = curve.clone();
        bad.points[1].celsius = f64::NAN;
        assert!(bad.validate(MIN_PERCENT).is_err());
        bad = curve.clone();
        bad.points.last_mut().unwrap().percent = 99;
        assert!(bad.validate(MIN_PERCENT).is_err());
    }
    #[test]
    fn all_presets_respect_calibrated_floor_and_thermal_ceiling() {
        for profile in Profile::ALL {
            for floor in MIN_PERCENT..=100 {
                let curve = Curve::preset(profile, floor);
                assert!(curve.validate(floor).is_ok());
                let mut previous = floor;
                for tenth in 1..=1100 {
                    let temperature = f64::from(tenth) / 10.;
                    let speed = curve.percent(temperature);
                    assert!(speed >= previous && speed >= floor && speed <= 100);
                    if temperature >= FULL_SPEED_TEMP {
                        assert_eq!(speed, 100);
                    }
                    previous = speed;
                }
                for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0., -10.] {
                    assert_eq!(Controller::new().next(&curve, invalid, Instant::now()), 100);
                }
            }
        }
    }
    #[test]
    fn ramps_down_only_after_dwell_and_rises_immediately() {
        let curve = Curve::preset(Profile::Auto, MIN_PERCENT);
        let t = Instant::now();
        let mut controller = Controller::new();
        assert_eq!(controller.next(&curve, 60., t), 100);
        assert_eq!(
            controller.next(&curve, 60., t + Duration::from_secs(29)),
            100
        );
        assert_eq!(
            controller.next(&curve, 60., t + Duration::from_secs(30)),
            95
        );
        assert_eq!(
            controller.next(&curve, 60., t + Duration::from_millis(30500)),
            95
        );
        assert_eq!(
            controller.next(&curve, 86., t + Duration::from_secs(31)),
            100
        );
    }
    #[test]
    fn calibration_is_bound_to_model_and_bios() {
        let mut c = Calibration {
            model: "Aspire A515-45".into(),
            bios: "V1.58".into(),
            minimum_percent: 40,
            version: 2,
            temperature_and_rpm_verified: true,
            independent_speed_verified: true,
            recovery_verified: true,
        };
        assert!(c.valid_for("Aspire A515-45", "V1.58"));
        assert!(!c.valid_for("Aspire A515-45", "V1.59"));
        c.recovery_verified = false;
        assert!(!c.valid_for("Aspire A515-45", "V1.58"));
    }
    #[test]
    fn overheating_requires_cooldown_even_after_profile_change() {
        let curve = Curve::preset(Profile::Quiet, MIN_PERCENT);
        let start = Instant::now();
        let mut controller = Controller::new();
        assert_eq!(controller.next(&curve, 86., start), 100);
        controller.reset();
        assert_eq!(
            controller.next(&curve, 76., start + Duration::from_secs(60)),
            100
        );
        assert_eq!(
            controller.next(&curve, 75., start + Duration::from_secs(61)),
            100
        );
        assert_eq!(
            controller.next(&curve, 75., start + Duration::from_secs(90)),
            100
        );
        assert!(controller.next(&curve, 75., start + Duration::from_secs(91)) < 100);
    }
}
