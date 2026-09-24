//! Pure Nia87 lighting codec, derived from the Pft -> CHe -> PB command path.
//! No device I/O is performed here. Reports exclude a host HID report-ID byte.

use serde::{Deserialize, Deserializer, Serialize};

pub const REPORT_LEN: usize = 64;
pub const LED_READ_COMMAND: u8 = 0x87;
pub const LED_WRITE_COMMAND: u8 = 0x07;
pub const USER_PICTURE_READ_COMMAND: u8 = 0x8c;
pub const USER_PICTURE_WRITE_COMMAND: u8 = 0x0c;
pub const PER_KEY_COLOR_WRITE_COMMAND: u8 = 0x14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub id: u8,
    pub name: &'static str,
    pub value: bool,
    pub speed: bool,
    pub rgb: bool,
    pub dazzle: bool,
    pub options: &'static [&'static str],
}

const NONE: &[&str] = &[];
const WAVE: &[&str] = &["right", "left", "down", "up"];
const SNAKE: &[&str] = &["z", "return"];
const KALEIDOSCOPE: &[&str] = &["out", "in"];
const LINE_WAVE: &[&str] = &["right", "left"];
const CIRCLE: &[&str] = &["anti-clockwise", "clockwise"];
const PICTURE: &[&str] = &["1", "2", "3"];
const MUSIC: &[&str] = &["upright", "separate", "intersect"];

macro_rules! effect {
    ($id:expr, $name:expr, $value:expr, $speed:expr, $rgb:expr, $dazzle:expr, $options:expr) => {
        Effect {
            id: $id,
            name: $name,
            value: $value,
            speed: $speed,
            rgb: $rgb,
            dazzle: $dazzle,
            options: $options,
        }
    };
}

/// Nia87's 22 advertised types, plus the inherited `LightOff` command.
pub const EFFECTS: &[Effect] = &[
    effect!(0, "LightOff", false, false, false, false, NONE),
    effect!(1, "LightAlwaysOn", true, false, true, true, NONE),
    effect!(2, "LightBreath", true, true, true, true, NONE),
    effect!(3, "LightNeon", true, true, false, false, NONE),
    effect!(4, "LightWave", true, true, true, true, WAVE),
    effect!(5, "LightRipple", true, true, true, true, NONE),
    effect!(6, "LightRaindrop", true, true, true, true, NONE),
    effect!(7, "LightSnake", true, true, true, true, SNAKE),
    effect!(8, "LightPressAction", true, true, true, true, NONE),
    effect!(9, "LightConverage", true, true, true, true, NONE),
    effect!(10, "LightSineWave", true, true, true, true, NONE),
    effect!(
        11,
        "LightKaleidoscope",
        true,
        true,
        true,
        true,
        KALEIDOSCOPE
    ),
    effect!(12, "LightLineWave", true, true, true, true, LINE_WAVE),
    effect!(13, "LightUserPicture", true, false, false, false, PICTURE),
    effect!(14, "LightLaser", true, true, true, true, NONE),
    effect!(15, "LightCircleWave", true, true, true, true, CIRCLE),
    effect!(16, "LightDazzing", true, true, true, true, NONE),
    effect!(17, "LightRainDown", true, true, true, true, NONE),
    effect!(18, "LightMeteor", true, true, true, true, NONE),
    effect!(19, "LightPressActionOff", true, true, true, true, NONE),
    effect!(20, "LightMusicFollow3", true, false, true, true, MUSIC),
    effect!(21, "LightScreenColor", false, false, false, false, NONE),
    effect!(22, "LightMusicFollow2", true, false, true, true, MUSIC),
];

pub fn effect_by_id(id: u8) -> Option<&'static Effect> {
    EFFECTS.iter().find(|effect| effect.id == id)
}

/// Raw LED response. Keeping all 64 bytes preserves unrecognized firmware data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Lighting {
    raw: Vec<u8>,
}

impl<'de> Deserialize<'de> for Lighting {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            raw: Vec<u8>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::decode(&wire.raw).map_err(serde::de::Error::custom)
    }
}

impl Lighting {
    pub fn decode(response: &[u8]) -> Result<Self, String> {
        if response.len() != REPORT_LEN {
            return Err(format!(
                "expected {REPORT_LEN} LED response bytes, got {}",
                response.len()
            ));
        }
        Ok(Self {
            raw: response.to_vec(),
        })
    }

    pub fn effect(&self) -> Option<&'static Effect> {
        effect_by_id(self.raw[1])
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn effect_id(&self) -> u8 {
        self.raw[1]
    }
    /// Context affecting the index-zero picture response on this firmware.
    pub fn picture_context(&self) -> [u8; 2] {
        [self.effect_id(), self.raw[4] >> 4]
    }
    pub fn speed(&self) -> Option<u8> {
        (self.raw[2] <= 4).then(|| 4 - self.raw[2])
    }
    pub fn value(&self) -> u8 {
        self.raw[3]
    }
    pub fn option_and_color_mode(&self) -> u8 {
        self.raw[4]
    }
    pub fn raw_rgb(&self) -> [u8; 3] {
        [self.raw[5], self.raw[6], self.raw[7]]
    }

    /// Interpret only catalog fields that can be represented without dropping
    /// unknown option bits. `raw()` remains available for every response.
    pub fn recognized_setting(&self) -> Option<LightingSetting> {
        let effect = self.effect()?;
        let option_index = self.raw[4] >> 4;
        let option = if effect.options.is_empty() {
            if option_index != 0 {
                return None;
            }
            None
        } else if (option_index as usize) < effect.options.len() {
            Some(option_index)
        } else {
            return None;
        };
        let low = self.raw[4] & 0x0f;
        let dazzle = match effect.id {
            13 if low == 0 => false,
            20 | 22 if low == 0 => true,
            20 | 22 if low == 4 => false,
            _ if low == 8 && effect.dazzle => true,
            _ if low == 7 => false,
            _ if low <= 6 && effect.rgb => false,
            _ => return None,
        };
        let raw_rgb = self.raw_rgb();
        let rgb = if effect.rgb {
            let color = if effect.id != 20 && effect.id != 22 && low <= 6 {
                const COMMON: [[u8; 3]; 7] = [
                    [255, 0, 0],
                    [255, 128, 0],
                    [255, 255, 0],
                    [0, 255, 0],
                    [0, 255, 255],
                    [0, 0, 255],
                    [255, 0, 255],
                ];
                COMMON[low as usize]
            } else if raw_rgb == [250, 255, 250] {
                [255, 255, 255]
            } else {
                raw_rgb
            };
            Some(color)
        } else {
            None
        };
        let setting = LightingSetting {
            effect_id: effect.id,
            value: effect.value.then_some(self.value()),
            speed: if effect.speed { self.speed() } else { None },
            option,
            rgb,
            dazzle,
        };
        validate(&setting).ok()?;
        Some(setting)
    }
}

/// Intent to write one global lighting mode. Optional fields follow the Nia87
/// mode catalog: a present field must be supplied and an absent field omitted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightingSetting {
    pub effect_id: u8,
    pub value: Option<u8>,
    pub speed: Option<u8>,
    /// Zero-based index into the effect's `options` list.
    pub option: Option<u8>,
    pub rgb: Option<[u8; 3]>,
    pub dazzle: bool,
}

fn validate(setting: &LightingSetting) -> Result<&'static Effect, String> {
    let effect = effect_by_id(setting.effect_id)
        .ok_or_else(|| format!("effect {} is not in Nia87's catalog", setting.effect_id))?;
    if setting.value.is_some() != effect.value || setting.value.is_some_and(|value| value > 4) {
        return Err("value must match this effect's catalog and be 0..=4".into());
    }
    if setting.speed.is_some() != effect.speed || setting.speed.is_some_and(|speed| speed > 4) {
        return Err("speed must match this effect's catalog and be 0..=4".into());
    }
    if effect.options.is_empty() {
        if setting.option.is_some() {
            return Err("this effect has no option".into());
        }
    } else if !setting
        .option
        .is_some_and(|option| (option as usize) < effect.options.len())
    {
        return Err(format!(
            "option must be an index in 0..{}",
            effect.options.len()
        ));
    }
    if setting.rgb.is_some() != effect.rgb || (setting.dazzle && !effect.dazzle) {
        return Err("RGB or dazzle is not supported by this effect".into());
    }
    Ok(effect)
}

fn checksum(report: &mut [u8; REPORT_LEN], index: usize) {
    let sum = report[..index]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    report[index] = 0xffu8.wrapping_sub(sum);
}

/// Read LED settings with the inherited BIT7 request. Response byte 0 should
/// not be used as a validity check; the vendor decoder reads bytes 1..7.
pub fn read_request() -> [u8; REPORT_LEN] {
    let mut report = [0; REPORT_LEN];
    report[0] = LED_READ_COMMAND;
    checksum(&mut report, 7);
    report
}

/// Build the inherited PB global setter using BIT8 framing.
pub fn write_report(setting: &LightingSetting) -> Result<[u8; REPORT_LEN], String> {
    let effect = validate(setting)?;
    let mut report = [0; REPORT_LEN];
    report[0] = LED_WRITE_COMMAND;
    report[1] = effect.id;
    report[2] = 4 - setting.speed.unwrap_or(0);
    report[3] = if effect.id == 21 {
        4
    } else {
        setting.value.unwrap_or(0)
    };
    let option = setting.option.unwrap_or(0) << 4;
    report[4] = match effect.id {
        13 => option,
        20 | 22 => option | if setting.dazzle { 0 } else { 4 },
        _ => option | if setting.dazzle { 8 } else { 7 },
    };
    if let Some(mut rgb) = setting.rgb {
        // The vendor substitutes a near-white sentinel for literal white.
        if rgb == [255, 255, 255] {
            rgb = [250, 255, 250];
        }
        report[5..8].copy_from_slice(&rgb);
    }
    if effect.id == 13 {
        report[5..8].copy_from_slice(&[0, 200, 200]);
    }
    checksum(&mut report, 8);
    Ok(report)
}

/// Six 64-byte pages yield 128 slot-indexed RGB triples. Their slot indices
/// are layout indices, not USB HID usages.
pub fn user_picture_read_requests() -> [[u8; REPORT_LEN]; 6] {
    std::array::from_fn(|page| {
        let mut report = [0; REPORT_LEN];
        report[0] = USER_PICTURE_READ_COMMAND;
        report[2] = page as u8;
        checksum(&mut report, 7);
        report
    })
}

pub fn user_picture_from_pages(pages: &[[u8; REPORT_LEN]]) -> Result<Vec<[u8; 3]>, String> {
    if pages.len() != 6 {
        return Err(format!(
            "expected six user-picture pages, got {}",
            pages.len()
        ));
    }
    let bytes: Vec<_> = pages.iter().flat_map(|page| page.iter().copied()).collect();
    Ok(bytes.as_chunks::<3>().0.to_vec())
}

/// The selected Nia87 picture writer uploads all 128 matrix-indexed RGB
/// triples in seven 56-byte pages. The final page has eight zero padding bytes.
/// Reports exclude the host's HID report-ID byte.
pub fn user_picture_write_reports(colors: &[[u8; 3]]) -> Result<[[u8; REPORT_LEN]; 7], String> {
    if colors.len() != 128 {
        return Err(format!(
            "expected 128 user-picture colors, got {}",
            colors.len()
        ));
    }
    let bytes: [u8; 384] = std::array::from_fn(|index| colors[index / 3][index % 3]);
    Ok(std::array::from_fn(|page| {
        let mut report = [0; REPORT_LEN];
        report[0] = USER_PICTURE_WRITE_COMMAND;
        report[2] = 0x80;
        report[3] = 1;
        report[4] = page as u8;
        checksum(&mut report, 7);
        let start = page * 56;
        let count = (bytes.len() - start).min(56);
        report[8..8 + count].copy_from_slice(&bytes[start..start + count]);
        report
    }))
}

/// Inherited CHe simple per-key color command. The caller must map a physical
/// key to its default-matrix index; passing a HID usage here would be wrong.
pub fn per_key_color_report(
    profile: u8,
    matrix_index: u8,
    rgb: [u8; 3],
) -> Result<[u8; REPORT_LEN], String> {
    if profile != 0 {
        return Err("Nia87 advertises only profile index 0".into());
    }
    if matrix_index >= 128 {
        return Err("matrix index must be 0..=127".into());
    }
    let mut report = [0; REPORT_LEN];
    report[0] = PER_KEY_COLOR_WRITE_COMMAND;
    report[1] = profile;
    report[2] = matrix_index;
    report[8..11].copy_from_slice(&rgb);
    checksum(&mut report, 7);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_use_inherited_commands_and_checksums() {
        assert_eq!(&read_request()[..9], &[0x87, 0, 0, 0, 0, 0, 0, 0x78, 0]);
        let picture = user_picture_read_requests();
        assert_eq!(&picture[0][..8], &[0x8c, 0, 0, 0, 0, 0, 0, 0x73]);
        assert_eq!(&picture[5][..8], &[0x8c, 0, 5, 0, 0, 0, 0, 0x6e]);
    }

    #[test]
    fn write_matches_pb_fields_and_rejects_unsupported_modes() {
        let setting = LightingSetting {
            effect_id: 4,
            value: Some(3),
            speed: Some(2),
            option: Some(3),
            rgb: Some([255, 255, 255]),
            dazzle: true,
        };
        let report = write_report(&setting).unwrap();
        assert_eq!(&report[..9], &[7, 4, 2, 3, 0x38, 250, 255, 250, 0xc4]);
        assert!(report[9..].iter().all(|byte| *byte == 0));
        assert_eq!(
            Lighting::decode(&report).unwrap().recognized_setting(),
            Some(setting.clone())
        );
        let mut invalid = setting;
        invalid.effect_id = 23;
        assert!(write_report(&invalid).is_err());
    }

    #[test]
    fn response_and_picture_pages_preserve_unknown_bytes() {
        let mut raw = [0; REPORT_LEN];
        raw[1] = 250;
        raw[63] = 0xa5;
        let lighting = Lighting::decode(&raw).unwrap();
        assert!(lighting.effect().is_none());
        assert_eq!(lighting.raw()[63], 0xa5);
        assert!(Lighting::decode(&raw[..63]).is_err());
        assert!(serde_json::from_str::<Lighting>("{\"raw\":[1,2,3]}").is_err());
        let pages = [[0xab; REPORT_LEN]; 6];
        let colors = user_picture_from_pages(&pages).unwrap();
        assert_eq!(colors.len(), 128);
        assert_eq!(colors[127], [0xab; 3]);
    }

    #[test]
    fn per_key_color_uses_layout_index_and_bit7() {
        let report = per_key_color_report(0, 125, [12, 34, 56]).unwrap();
        assert_eq!(&report[..8], &[20, 0, 125, 0, 0, 0, 0, 0x6e]);
        assert_eq!(&report[8..11], &[12, 34, 56]);
        assert!(per_key_color_report(1, 0, [0; 3]).is_err());
        assert!(per_key_color_report(0, 128, [0; 3]).is_err());
    }

    #[test]
    fn bulk_picture_reports_have_exact_headers_and_roundtrip_rgb_across_pages() {
        let colors: Vec<_> = (0..128)
            .map(|slot| [slot as u8, (slot + 1) as u8, 255 - slot as u8])
            .collect();
        let reports = user_picture_write_reports(&colors).unwrap();
        for (page, report) in reports.iter().enumerate() {
            assert_eq!(
                &report[..8],
                &[0x0c, 0, 0x80, 1, page as u8, 0, 0, 0x72 - page as u8]
            );
        }
        let flat: Vec<_> = colors.iter().flatten().copied().collect();
        assert_eq!(reports[0][63], flat[55]);
        assert_eq!(reports[1][8], flat[56]);
        assert_eq!(reports[6][8..56], flat[336..384]);
        assert_eq!(&reports[6][56..], &[0; 8]);
        let uploaded: Vec<_> = reports
            .iter()
            .flat_map(|report| report[8..].iter().copied())
            .take(384)
            .collect();
        assert_eq!(uploaded, flat);
    }

    #[test]
    fn bulk_picture_requires_exactly_128_rgb_triples() {
        assert!(user_picture_write_reports(&[[0; 3]; 127]).is_err());
        assert!(user_picture_write_reports(&[[0; 3]; 129]).is_err());
    }
}
