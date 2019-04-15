// This program is free software: you can redistribute it and/or modify
// it under the terms of the Lesser GNU General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// Lesser GNU General Public License for more details.

// You should have received a copy of the Lesser GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

// Copyright 2019 E-Nguyen Developers.

use crate::ewin;

use lazy_static::lazy_static;
use log::{error, warn};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use toml;
use vulkano::instance::PhysicalDevice;

static DEFAULT_CONF_DIR: &str = "~/.config/e-nguyen/";
static DEFAULT_TOML_FILE: &str = "e-nguyen.toml";

lazy_static! {
    static ref CONFIG_LOCK: Mutex<()> = Mutex::new(());
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ENguyenConfig {
    pub physical_device_index: i32,
    pub audio_input_index: i32,
    pub start_in_fullscreen: bool,
}

impl ENguyenConfig {
    pub fn parse(path: &PathBuf) -> Result<ENguyenConfig, Box<Error>> {
        let mut config_toml = String::new();
        {
            let _held = CONFIG_LOCK.lock();
            let mut f = File::open(path)?;
            f.read_to_string(&mut config_toml)?;
        }
        let parsed = toml::from_str(&config_toml)?;
        Ok(parsed)
    }

    pub fn save(&self, path_override: Option<PathBuf>) -> Result<(), Box<Error>> {
        let path = path_override.unwrap_or_else(|| default_config_path());
        let mut cloned = path.clone();
        cloned.pop();
        {
            let _held = CONFIG_LOCK.lock();
            std::fs::create_dir_all(cloned)?;
            let as_toml = toml::to_string_pretty(self)?;
            let mut f = File::create(path)?;
            f.write(as_toml.as_bytes())?;
        }
        Ok(())
    }

    pub fn ready(&self, picker: &ewin::GpuPicker) -> bool {
        // TODO UUID is more robust for verifying that config is referring to same GPU as before
        match PhysicalDevice::from_index(&picker.instance, self.physical_device_index as usize) {
            None => {
                warn!("The configured physical device doesn't exist.  Update your settings");
                warn!("Proceeding with a default configuration.");
                false
            }
            Some(pd) => ewin::GpuPicker::has_graphics(&pd),
        }
    }
}

impl Default for ENguyenConfig {
    fn default() -> Self {
        ENguyenConfig {
            start_in_fullscreen: false,
            physical_device_index: 0,
            audio_input_index: -1,
        }
    }
}

pub fn try_load_config(path: PathBuf) -> Option<ENguyenConfig> {
    match ENguyenConfig::parse(&path) {
        Ok(parsed) => Some(parsed),
        Err(failed) => {
            error!("Failed to parse config! [{}]", failed);
            None
        }
    }
}

fn default_config_path() -> PathBuf {
    // TODO platform independence
    let mut p = PathBuf::from(DEFAULT_CONF_DIR);
    p.push(DEFAULT_TOML_FILE);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instantiate_default() {
        let en_conf = ENguyenConfig::default();
        assert_eq!(en_conf.start_in_fullscreen, false);
    }

    #[test]
    fn get_default_path() {
        default_config_path();
    }

    #[test]
    fn save_config() {
        let en_conf = ENguyenConfig::default();
        let mut test_path = std::env::temp_dir();
        test_path.push(format!("test_{}", DEFAULT_TOML_FILE));
        en_conf.save(Some(test_path)).unwrap();
    }

    #[test]
    fn load_config() {
        let mut en_conf = ENguyenConfig::default();
        en_conf.start_in_fullscreen = true;
        let mut test_path = std::env::temp_dir();
        test_path.push(format!("test_{}", DEFAULT_TOML_FILE));
        en_conf.save(Some(test_path.clone())).unwrap();
        let loaded = ENguyenConfig::parse(&test_path);
        assert_eq!(loaded.unwrap().start_in_fullscreen, true);
    }

    #[test]
    fn test_ready() {
        use crate::ewin::GpuPicker;
        let en_conf = ENguyenConfig::default();
        let picker = GpuPicker::new(false);
        assert!(en_conf.ready(&picker.unwrap()));
    }
}
