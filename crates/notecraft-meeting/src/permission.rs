//! The macOS microphone permission: ask for it explicitly and tell why it is missing.

/// Where the permission stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicAccess {
    /// macOS has never been asked: [`request`] shows its window.
    NotDetermined,
    /// The person said no (or it is blocked by a profile).
    Denied,
    Authorized,
}

#[cfg(target_os = "macos")]
pub fn status() -> MicAccess {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    let Some(kind) = (unsafe { AVMediaTypeAudio }) else { return MicAccess::Authorized };
    let s = unsafe { AVCaptureDevice::authorizationStatusForMediaType(kind) };
    if s == AVAuthorizationStatus::Authorized {
        MicAccess::Authorized
    } else if s == AVAuthorizationStatus::NotDetermined {
        MicAccess::NotDetermined
    } else {
        MicAccess::Denied
    }
}

/// Shows the macOS "allow the microphone" window (does nothing if it was already answered).
#[cfg(target_os = "macos")]
pub fn request() {
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    let Some(kind) = (unsafe { AVMediaTypeAudio }) else { return };
    let handler = block2::RcBlock::new(|_granted: objc2::runtime::Bool| {});
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(kind, &handler) };
}

#[cfg(not(target_os = "macos"))]
pub fn status() -> MicAccess {
    MicAccess::Authorized
}

#[cfg(not(target_os = "macos"))]
pub fn request() {}
