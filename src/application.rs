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

use crate::config::ENguyenConfig;
use crate::ewin::GpuPicker;
use crate::mesmerize;
use crate::settings;

use log::{error, info};
use std::sync::mpsc;
use std::sync::mpsc::SyncSender;
use std::thread;
use std::thread::JoinHandle;

#[derive(Debug)]
enum Message {
    LaunchMez,
    LaunchSettings,
    ClosedMez,
    ClosedSettings,
}

pub enum LaunchRequest {
    Mez,
    Settings,
}

pub struct MezLauncher {
    sender: SyncSender<Message>,
    pub picker: GpuPicker,
}

impl MezLauncher {
    fn launch(self) {
        let r = mesmerize::mezmerize(&self);
        match r {
            Err(e) => error!("{:?}", e),
            _ => {}
        }
        self.sender.send(Message::ClosedMez).unwrap();
    }

    pub fn launch_settings(&self) {
        self.sender.send(Message::LaunchSettings).unwrap();
    }
}

pub struct SettingsLauncher {
    sender: SyncSender<Message>,
    pub picker: GpuPicker,
}

impl SettingsLauncher {
    fn launch(self) {
        settings::settings_ui(&self).unwrap();
        info!("Finished setting");
        self.sender.send(Message::ClosedSettings).unwrap();
    }

    pub fn launch_mez(&self) {
        self.sender.send(Message::LaunchMez).unwrap();
    }
}

pub struct App {
    settings_handle: Option<JoinHandle<()>>,
    mez_handle: Option<JoinHandle<()>>,
}

impl App {
    pub fn new() -> App {
        App { settings_handle: None, mez_handle: None }
    }

    fn settings_alive(&self) -> bool {
        match self.settings_handle {
            Some(_) => true,
            None => false,
        }
    }

    fn mez_alive(&self) -> bool {
        match self.mez_handle {
            Some(_) => true,
            None => false,
        }
    }

    fn launch_settings(&mut self, tx: &SyncSender<Message>, picker: GpuPicker) {
        if !self.settings_alive() {
            let settings = SettingsLauncher { sender: tx.clone(), picker };
            self.settings_handle = Some(thread::spawn(move || {
                settings.launch();
            }));
        }
    }

    fn launch_mez(&mut self, tx: &SyncSender<Message>, picker: GpuPicker) {
        if !self.mez_alive() {
            let mez = MezLauncher { sender: tx.clone(), picker };
            self.mez_handle = Some(thread::spawn(move || {
                mez.launch();
            }));
        }
    }

    pub fn launch(request: LaunchRequest, _config: ENguyenConfig, picker: GpuPicker) {
        let (tx, rx) = mpsc::sync_channel(5);
        let mut app = App::new();
        match request {
            LaunchRequest::Settings => {
                app.launch_settings(&tx, picker.clone());
            }
            LaunchRequest::Mez => {
                app.launch_mez(&tx, picker.clone());
            }
        }

        for recieved in rx.iter() {
            match recieved {
                Message::LaunchMez => {
                    app.launch_mez(&tx, picker.clone());
                }
                Message::LaunchSettings => {
                    app.launch_settings(&tx, picker.clone());
                }
                Message::ClosedSettings => {
                    if let Some(handle) = app.settings_handle {
                        handle.join().expect("Could not join Settings thread");
                        app.settings_handle = None
                    }
                    if !app.mez_alive() {
                        break;
                    }
                }
                Message::ClosedMez => {
                    if let Some(handle) = app.mez_handle {
                        handle.join().expect("Could not join Mezmerizer thread");
                        app.mez_handle = None
                    }
                    if !app.settings_alive() {
                        break;
                    }
                }
            };
        }
    }
}
