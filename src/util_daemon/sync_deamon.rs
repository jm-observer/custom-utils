use std::thread::JoinHandle;

#[cfg(not(target_os = "linux"))]
pub fn daemon() -> JoinHandle<()> {
    std::thread::spawn(|| {})
}
#[cfg(target_os = "linux")]
pub fn daemon() -> JoinHandle<()> {
    std::thread::spawn(|| {
        use libsystemd::daemon::{self, NotifyState};
        use log::{debug, error, info, warn};
        use std::time::Duration;
        if !daemon::booted() {
            info!("Not running systemd, early exit.");
            return;
        };
        // `Type=notify` units must receive `READY=1` to become active. Send it
        // before the watchdog-enabled check so readiness never depends on
        // `WatchdogSec=` being set, otherwise the unit start always times out.
        if let Err(err) = daemon::notify(false, &[NotifyState::Ready]) {
            error!("daemon ready notify error: {:?}", err);
        }
        let timeout = match daemon::watchdog_enabled(true) {
            Some(time) => time,
            None => {
                info!("watchdog_enabled false");
                return;
            }
        };
        let mut sleep_sec = timeout.as_secs();
        debug!("daemon timeout = {}s", sleep_sec);
        if sleep_sec <= 15 {
            warn!("watchdog timeout <= 10s");
            return;
        }
        sleep_sec -= 10;
        loop {
            match daemon::notify(false, &[NotifyState::Watchdog]) {
                Ok(_res) => {
                    debug!("deamon::notify true");
                }
                Err(err) => {
                    error!("daemon error: {:?}", err);
                    break;
                }
            }
            std::thread::sleep(Duration::from_secs(sleep_sec));
        }
    })
}
