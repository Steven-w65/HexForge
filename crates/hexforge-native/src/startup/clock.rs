//! Process-creation/QPC correlation. Absolute FILETIME and QPC values are never
//! cast to floating point; only exact, local differences become milliseconds.
use super::presentation::{MeasurementError, ProcessTimeline, Tick};

/// Correlate the OS process creation FILETIME to a wall-clock sample bracketed
/// by QPC reads. FILETIME is measured in 100ns ticks; the QPC frequency is explicit.
/// Uncertainty includes half the bracket (rounded up) and one conversion tick.
pub fn correlate_process_creation(
    process_id: u32,
    creation_100ns: u64,
    sampled_100ns: u64,
    before: Tick,
    after: Tick,
) -> Result<ProcessTimeline, MeasurementError> {
    let frequency = before.frequency()?;
    after.frequency()?;
    if before.domain != after.domain {
        return Err(MeasurementError::UncorrelatedClock);
    }
    let age_100ns = sampled_100ns
        .checked_sub(creation_100ns)
        .ok_or(MeasurementError::InvalidInterval)?;
    let span = after
        .ticks
        .checked_sub(before.ticks)
        .filter(|v| *v >= 0)
        .ok_or(MeasurementError::InvalidInterval)?;
    let midpoint = (i128::from(before.ticks) + i128::from(after.ticks)) / 2;
    let age_qpc = u128::from(age_100ns)
        .checked_mul(u128::from(frequency))
        .ok_or(MeasurementError::InvalidInterval)?
        / 10_000_000;
    let origin = midpoint
        .checked_sub(i128::try_from(age_qpc).map_err(|_| MeasurementError::InvalidInterval)?)
        .ok_or(MeasurementError::InvalidInterval)?;
    let origin = i64::try_from(origin).map_err(|_| MeasurementError::InvalidInterval)?;
    if origin < 0 {
        return Err(MeasurementError::InvalidInterval);
    }
    let uncertainty = (span as u64)
        .div_ceil(2)
        .checked_add(1)
        .ok_or(MeasurementError::InvalidInterval)?;
    ProcessTimeline::new(process_id, Tick::qpc(origin, frequency), uncertainty)
}

#[derive(Debug)]
pub enum ClockError {
    Api(std::io::Error),
    Evidence(MeasurementError),
    UnsupportedPlatform,
}
impl std::fmt::Display for ClockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(e) => write!(f, "Windows startup clock failed: {e}"),
            Self::Evidence(e) => e.fmt(f),
            Self::UnsupportedPlatform => f.write_str(
                "Process-creation/QPC startup measurement is only supported on Windows.",
            ),
        }
    }
}
impl std::error::Error for ClockError {}
impl From<MeasurementError> for ClockError {
    fn from(value: MeasurementError) -> Self {
        Self::Evidence(value)
    }
}

/// Constructed only for opt-in diagnostics. Normal startup never calls these
/// APIs. A failing API is an explicit measurement failure, not an Instant fallback.
#[derive(Clone, Debug)]
pub struct NativeClock {
    frequency: u64,
}
impl NativeClock {
    #[cfg(windows)]
    pub fn new() -> Result<Self, ClockError> {
        let mut frequency = 0;
        if unsafe {
            windows_sys::Win32::System::Performance::QueryPerformanceFrequency(&mut frequency)
        } == 0
            || frequency <= 0
        {
            return Err(ClockError::Api(std::io::Error::last_os_error()));
        }
        Ok(Self {
            frequency: frequency as u64,
        })
    }
    #[cfg(not(windows))]
    pub fn new() -> Result<Self, ClockError> {
        Err(ClockError::UnsupportedPlatform)
    }

    #[cfg(windows)]
    pub fn now(&self) -> Result<Tick, ClockError> {
        let mut counter = 0;
        if unsafe { windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut counter) }
            == 0
            || counter < 0
        {
            return Err(ClockError::Api(std::io::Error::last_os_error()));
        }
        Ok(Tick::qpc(counter, self.frequency))
    }
    #[cfg(not(windows))]
    pub fn now(&self) -> Result<Tick, ClockError> {
        Err(ClockError::UnsupportedPlatform)
    }

    #[cfg(windows)]
    pub fn current_process_timeline(&self) -> Result<ProcessTimeline, ClockError> {
        self.process_timeline(std::process::id())
    }
    /// The passive development observer correlates the launched process's OS
    /// creation, not the observer's attach time or entry into either Rust main.
    #[cfg(windows)]
    pub fn process_timeline(&self, process_id: u32) -> Result<ProcessTimeline, ClockError> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, FILETIME, HANDLE},
            System::{
                SystemInformation::GetSystemTimePreciseAsFileTime,
                Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
            },
        };
        fn value(t: FILETIME) -> u64 {
            (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
        }
        if process_id == 0 {
            return Err(MeasurementError::WrongFrameIdentity.into());
        }
        struct Process(HANDLE);
        impl Drop for Process {
            fn drop(&mut self) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if handle.is_null() {
            return Err(ClockError::Api(std::io::Error::last_os_error()));
        }
        let process = Process(handle);
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        if unsafe { GetProcessTimes(process.0, &mut creation, &mut exit, &mut kernel, &mut user) }
            == 0
        {
            return Err(ClockError::Api(std::io::Error::last_os_error()));
        }
        let before = self.now()?;
        let mut sampled = FILETIME::default();
        unsafe { GetSystemTimePreciseAsFileTime(&mut sampled) };
        let after = self.now()?;
        Ok(correlate_process_creation(
            process_id,
            value(creation),
            value(sampled),
            before,
            after,
        )?)
    }
    #[cfg(not(windows))]
    pub fn current_process_timeline(&self) -> Result<ProcessTimeline, ClockError> {
        Err(ClockError::UnsupportedPlatform)
    }
    #[cfg(not(windows))]
    pub fn process_timeline(&self, _process_id: u32) -> Result<ProcessTimeline, ClockError> {
        Err(ClockError::UnsupportedPlatform)
    }
}
