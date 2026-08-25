use std::fs::File;
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use nix::ioctl_write_int_bad;
use anyhow::Error;
use super::{Frontlight, LightLevels};

ioctl_write_int_bad!(write_frontlight_intensity, 241);
const FRONTLIGHT_INTERFACE: &str = "/dev/ntx_io";

pub struct StandardFrontlight {
    value: f32,
    // The intensity that was last written to the interface.
    written: Option<libc::c_int>,
    interface: File,
}

impl StandardFrontlight {
    pub fn new(value: f32) -> Result<StandardFrontlight, Error> {
        let interface = OpenOptions::new().write(true)
                                    .open(FRONTLIGHT_INTERFACE)?;
        Ok(StandardFrontlight { value, written: None, interface })
    }
}

impl Frontlight for StandardFrontlight {
    fn set_intensity(&mut self, value: f32) {
        let intensity = value as libc::c_int;
        // This ioctl is slow, hence we avoid the redundant calls.
        if self.written == Some(intensity) {
            self.value = value;
            return;
        }
        let ret = unsafe {
            write_frontlight_intensity(self.interface.as_raw_fd(), intensity)
        };
        if ret.is_ok() {
            self.value = value;
            self.written = Some(intensity);
        }
    }

    fn set_warmth(&mut self, _value: f32) { }

    fn levels(&self) -> LightLevels {
        LightLevels {
            intensity: self.value,
            warmth: 0.0,
        }
    }
}
