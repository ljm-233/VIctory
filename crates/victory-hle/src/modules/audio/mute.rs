use std::time::Duration;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::core::{Interface, Result};

/// Prime this process's default audio sessions before the title runs, then enforce mute
/// for its additional sessions and newly connected playback devices.
pub fn start() {
    if std::env::var_os("VICTORY_FORCE_MUTE").is_none_or(|v| v != "1") {
        return;
    }
    let (ready, first) = std::sync::mpsc::channel();
    let worker = std::thread::Builder::new().name("victory-game-mute".into()).spawn(move || {
        // SAFETY: this dedicated thread owns its COM apartment for its entire lifetime.
        if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
            return;
        }
        let mut ready = Some(ready);
        let mut reported = false;
        loop {
            if let Ok((primed, active)) = enforce() {
                if !reported && active > 0 {
                    tracing::info!(pid = std::process::id(), active, "game audio session confirmed muted");
                    reported = true;
                }
                if primed > 0 && !reported {
                    tracing::debug!(primed, "default game audio session muted");
                }
            }
            if let Some(sender) = ready.take() {
                let _ = sender.send(());
            }
            std::thread::sleep(Duration::from_millis(400));
        }
    });
    if worker.is_ok() {
        let _ = first.recv_timeout(Duration::from_secs(2));
    }
}

fn mute(volume: &ISimpleAudioVolume) -> Result<bool> {
    // SAFETY: a live session volume interface on this thread, with no event context.
    unsafe {
        if !volume.GetMute()?.as_bool() {
            volume.SetMute(true, std::ptr::null())?;
        }
        Ok(volume.GetMute()?.as_bool())
    }
}

fn enforce() -> Result<(u32, u32)> {
    use windows::Win32::Media::Audio::AudioSessionStateActive;
    // SAFETY: COM is initialized on this thread. Only process-local default sessions or
    // sessions positively matching our PID are modified, never endpoint master volume.
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let endpoints = devices.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let mut primed = 0;
        let mut active = 0;
        for i in 0..endpoints.GetCount()? {
            let Ok(endpoint) = endpoints.Item(i) else { continue };
            let Ok(manager) = endpoint.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            if let Ok(default) = manager.GetSimpleAudioVolume(None, 0)
                && mute(&default).unwrap_or(false) { primed += 1; }
            let Ok(sessions) = manager.GetSessionEnumerator() else { continue };
            for j in 0..sessions.GetCount().unwrap_or(0) {
                let Ok(session) = sessions.GetSession(j) else { continue };
                let Ok(owner) = session.cast::<IAudioSessionControl2>() else { continue };
                if owner.GetProcessId().ok() != Some(std::process::id()) { continue; }
                if let Ok(volume) = session.cast::<ISimpleAudioVolume>()
                    && mute(&volume).unwrap_or(false)
                    && session.GetState().ok() == Some(AudioSessionStateActive) { active += 1; }
            }
        }
        Ok((primed, active))
    }
}