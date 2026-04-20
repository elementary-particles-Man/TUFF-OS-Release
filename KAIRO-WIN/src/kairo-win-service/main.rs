#[cfg(windows)]
use std::{
    ffi::OsString,
    sync::mpsc,
    time::Duration,
};

#[cfg(windows)]
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlHandlerResult, ServiceStatus, ServiceType,
        ServiceExitCode, ServiceState,
    },
    service_control_handler::{self, ServiceControlHandlerEvents},
    service_dispatcher,
};

#[cfg(windows)]
define_windows_service!(ffi_service_main, kairo_service_main);

mod wfp_shim;

use anyhow::Result;
use tokio::runtime::Runtime;

fn main() -> Result<(), windows_service::Error> {
    #[cfg(windows)]
    {
        service_dispatcher::start("kairo-win-service", ffi_service_main)?;
    }
    #[cfg(not(windows))]
    {
        println!("This application is designed for Windows.");
    }
    Ok(())
}

#[cfg(windows)]
pub fn kairo_service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service() {
        eprintln!("Service error: {}", e);
    }
}

#[cfg(windows)]
fn run_service() -> Result<()> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControlHandlerEvents::Stop => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControlHandlerEvents::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register("kairo-win-service", event_handler)?;

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControl::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    // Create tokio runtime
    let rt = Runtime::new()?;
    rt.block_on(async {
        // Initialize WFP Shim
        let wfp = match wfp_shim::WfpShim::new() {
            Ok(w) => {
                let _ = w.apply_kairo_filter();
                Some(w)
            }
            Err(e) => {
                log::error!("Failed to initialize WFP Shim: {}", e);
                None
            }
        };

        // Run the daemon logic
        tokio::spawn(async {
            // Note: In a real implementation, we would pass the WFP handle or similar if needed
            if let Err(e) = kairo_win_service::run_embedded_daemon().await {
                log::error!("Daemon error: {}", e);
            }
        });

        // Wait for shutdown signal from Service Control Manager
        while shutdown_rx.try_recv().is_err() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        log::info!("Shutting down KAIRO-WIN service...");
        drop(wfp);
    });

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControl::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}
