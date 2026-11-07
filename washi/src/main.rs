#![windows_subsystem = "windows"]

use std::rc::Rc;
use washi::platform::Dialogs;
use washi::win::*;
use washi::{App, Env};

fn main() {
    let dialogs = Rc::new(Win32Dialogs);
    let Some(appdata) = std::env::var_os("APPDATA") else {
        dialogs.error("Washi could not find your AppData folder.");
        return;
    };
    let env = Env {
        data_dir: std::path::PathBuf::from(appdata).join("Washi"),
        protector: Rc::new(DpapiProtector),
        clock: Rc::new(SystemClock::new()),
        signals: Rc::new(Win32Signals::default()),
        power: Rc::new(Win32Power),
        menu: Rc::new(Win32ContextMenu),
        dialogs: dialogs.clone(),
    };
    match App::start(env) {
        Ok(app) => {
            let _ = slint::run_event_loop_until_quit();
            app.quit(); // final flush
        }
        Err(e) => dialogs.error(&e.to_string()),
    }
}
