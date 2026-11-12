#![windows_subsystem = "windows"]

use std::rc::Rc;
use washi::platform::Dialogs;
use washi::win::*;
use washi::{App, Env};

fn main() {
    washi::trace::mark("process-start");
    let dialogs = Rc::new(Win32Dialogs);
    // WASHI_DATA_DIR points the app at a throwaway profile (tools/memprobe.ps1, tools/capture.ps1).
    let data_dir = match (std::env::var_os("WASHI_DATA_DIR"), std::env::var_os("APPDATA")) {
        (Some(dir), _) => std::path::PathBuf::from(dir),
        (None, Some(appdata)) => std::path::PathBuf::from(appdata).join("Washi"),
        (None, None) => {
            dialogs.error("Washi could not find your AppData folder.");
            return;
        }
    };
    let env = Env {
        data_dir,
        protector: Rc::new(DpapiProtector),
        clock: Rc::new(SystemClock::new()),
        signals: Rc::new(Win32Signals::default()),
        power: Rc::new(Win32Power),
        startup: Rc::new(Win32Startup),
        instance: Rc::new(Win32Instance),
        dialogs: dialogs.clone(),
        clipboard: Rc::new(Win32Clipboard),
        window_control: Rc::new(Win32WindowControl),
        stats: Rc::new(Win32Stats),
        shell: Rc::new(Win32Shell),
    };
    set_app_identity();
    match App::launch(env) {
        Ok(Some(app)) => {
            app.watch_window_state();
            washi::trace::mark("notes-shown");
            slint::Timer::single_shot(std::time::Duration::ZERO, || washi::trace::mark("first-turn"));
            let _ = slint::run_event_loop_until_quit();
            app.quit(); // final flush
        }
        Ok(None) => {} // handed off to the running Washi
        Err(e) => dialogs.error(&e.to_string()),
    }
}
