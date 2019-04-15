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

mod application;
mod audio;
mod compute;
mod config;
mod errors;
mod ewin;
mod input;
mod mesmerize;
mod rendering;
mod ring;
mod settings;

use crate::application::{App, LaunchRequest};

use docopt::Docopt;
use env_logger::{Builder, Target};
use log::{error, info, warn, LevelFilter};
use serde::Deserialize;
use std::path::PathBuf;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");
const USAGE: &'static str = "
E-Nguyen

Usage:
  e-nguyen [options]
  e-nguyen (-h | --help)
  e-nguyen --version

Options:
  -h --help           Show this screen
  -v --version        Show version
  -c --config PATH    Custom configuration path
  -f --fullscreen     Start in fullscreen
  -l --layers         Enable Vulkan debug layers
  -b --buffers        Enable robust buffer access
  --verbose           RUST_LOG=debug
";
const VERSION_BANNER_TEMPLATE: &'static str = r"
 ___   __  _  __ _  ___   _____ __  _   
| __|_|  \| |/ _] || \ `v' / __|  \| |  
| _|__| | ' | [/\ \/ |`. .'| _|| | ' |  
|___| |_|\__|\__/\__/  !_! |___|_|\__|  

Version: ☃
Copyright 2019
Made available under the GNU LGPL-3.0 License
";

#[derive(Debug, Deserialize)]
struct Args {
    flag_config: String,
    flag_fullscreen: bool,
    flag_layers: bool,
    flag_version: bool,
    flag_verbose: bool,
}

fn main() {
    let args: Args = Docopt::new(USAGE).and_then(|d| d.deserialize()).unwrap_or_else(|e| e.exit());
    let mut builder = Builder::default();
    builder.target(Target::Stdout);
    if args.flag_verbose {
        builder.filter_level(LevelFilter::Trace);
    } else {
        builder.filter_level(LevelFilter::Warn);
    }
    builder.init();

    if args.flag_version {
        let parts: Vec<&str> = VERSION_BANNER_TEMPLATE.split("☃").collect();
        let version_string: String = parts.join(VERSION);
        println!("{}", version_string);
        std::process::exit(0)
    }

    let config = {
        let mut parsed = None;
        if !args.flag_config.is_empty() {
            let args_path = args.flag_config.clone();
            info!("Loading custom configuration from {}", args_path);
            let path = PathBuf::from(&args_path);
            if path.exists() && path.is_file() {
                parsed = config::try_load_config(path);
            } else {
                warn!("Invalid config path! {}", args_path);
            }
        }
        match parsed {
            Some(c) => c,
            None => config::ENguyenConfig::default(),
        }
    };

    let load_layers = args.flag_layers;
    let picker = match ewin::GpuPicker::new(load_layers) {
        Ok(i) => i,
        Err(_) => {
            error!("Missing Vulkan loader, ICD, or Vulkan capable device");
            error!("https://vulkan.lunarg.com/doc/view/1.0.54.0/windows/LoaderAndLayerInterface.html#Overview");
            error!("Consult your operating system and graphics card documentation for ICD & Vulkan loader installation instructions");
            std::process::exit(66);
        }
    };

    let skip_settings = args.flag_fullscreen || config.start_in_fullscreen;
    if skip_settings && config.ready(&picker) {
        App::launch(LaunchRequest::Mez, config, picker);
    } else {
        App::launch(LaunchRequest::Settings, config, picker);
    }
}
