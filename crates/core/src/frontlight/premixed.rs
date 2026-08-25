use std::path::Path;
use std::io::Write;
use std::fs::File;
use std::fs::OpenOptions;
use anyhow::Error;
use crate::device::CURRENT_DEVICE;
use super::{Frontlight, LightLevels};

const FRONTLIGHT_WHITE: &str = "/sys/class/backlight/mxc_msp430.0/brightness";

// Forma
const FRONTLIGHT_ORANGE_A: &str = "/sys/class/backlight/tlc5947_bl/color";
// Libra H₂O, Clara HD, Libra 2, Clara BW, Libra Colour, Clara Colour
const FRONTLIGHT_ORANGE_B: &str = "/sys/class/backlight/lm3630a_led/color";
// Sage, Libra 2, Clara 2E, Elipsa 2E
const FRONTLIGHT_ORANGE_C: &str =  "/sys/class/leds/aw99703-bl_FL1/color";

pub struct PremixedFrontlight {
    intensity: f32,
    warmth: f32,
    white: File,
    orange: File,
    // The values that were last written to the interface.
    written_white: Option<i16>,
    written_orange: Option<i16>,
}

impl PremixedFrontlight {
    pub fn new(intensity: f32, warmth: f32) -> Result<PremixedFrontlight, Error> {
        let white = OpenOptions::new().write(true).open(FRONTLIGHT_WHITE)?;
        let orange_path = if Path::new(FRONTLIGHT_ORANGE_C).exists() {
            FRONTLIGHT_ORANGE_C
        } else if Path::new(FRONTLIGHT_ORANGE_B).exists() {
            FRONTLIGHT_ORANGE_B
        } else {
            FRONTLIGHT_ORANGE_A
        };
        let orange = OpenOptions::new().write(true).open(orange_path)?;
        Ok(PremixedFrontlight { intensity, warmth, white, orange,
                                written_white: None, written_orange: None })
    }
}

impl Frontlight for PremixedFrontlight {
    fn set_intensity(&mut self, intensity: f32) {
        let white = intensity.round() as i16;
        // Writing to this file is slow, hence we avoid the redundant writes.
        if self.written_white != Some(white) {
            write!(self.white, "{}", white).unwrap();
            self.written_white = Some(white);
        }
        self.intensity = intensity;
    }

    fn set_warmth(&mut self, warmth: f32) {
        let mut orange = (warmth / 10.0).round() as i16;
        if CURRENT_DEVICE.mark() != 8 {
            orange = 10 - orange;
        }
        if self.written_orange != Some(orange) {
            write!(self.orange, "{}", orange).unwrap();
            self.written_orange = Some(orange);
        }
        self.warmth = warmth;
    }

    fn levels(&self) -> LightLevels {
        LightLevels {
            intensity: self.intensity,
            warmth: self.warmth,
        }
    }
}
