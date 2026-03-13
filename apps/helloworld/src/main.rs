#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start() -> ! {
    ulib::sys_log("helloworld: Service started, sending IPC to serial...");
    
    // Send IPC to serial service (assuming dummy PID 0 for broadcast/serial for now)
    let msg = b"Hello World from IPC!!";
    ulib::sys_ipc_send(0, msg);
    
    ulib::sys_log("helloworld: IPC sent. Exiting.");
    ulib::sys_exit(0);
}
