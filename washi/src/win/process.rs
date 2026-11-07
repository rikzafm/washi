use crate::platform::ProcessPower;
use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapCompact};
use windows_sys::Win32::System::ProcessStatus::K32EmptyWorkingSet;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, ProcessPowerThrottling, SetPriorityClass, SetProcessInformation, IDLE_PRIORITY_CLASS,
    NORMAL_PRIORITY_CLASS, PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_STATE,
};

pub struct Win32Power;

impl ProcessPower for Win32Power {
    fn set_efficiency_mode(&self, on: bool) {
        // SAFETY: the state struct is fully initialised and the size matches.
        unsafe {
            let process = GetCurrentProcess();
            let state = PROCESS_POWER_THROTTLING_STATE {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
                StateMask: if on { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
            };
            SetProcessInformation(
                process,
                ProcessPowerThrottling,
                &state as *const _ as *const _,
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            );
            SetPriorityClass(process, if on { IDLE_PRIORITY_CLASS } else { NORMAL_PRIORITY_CLASS });
        }
    }

    fn release_memory(&self) {
        // SAFETY: plain process-memory calls on the current process.
        unsafe {
            HeapCompact(GetProcessHeap(), 0);
            K32EmptyWorkingSet(GetCurrentProcess());
        }
    }
}
