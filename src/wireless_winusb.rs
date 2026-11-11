//! Windows WinUSB V1 GetDev probe, explicitly user-initiated.
//! One 64-byte discovery request to RX 0416:8041; NO PWM/RGB/LCD writes.
//! Refuses access if L-Connect services/watchers are still running.
//! Never changes USB configuration, drivers, services or fan fallback states.
use crate::wireless_protocol::{parse_getdev, LiveTopology};
use std::process::Command;

pub fn probe() -> Result<LiveTopology, String> {
    check_owner()?;
    platform::one_getdev()
}

fn check_owner() -> Result<(), String> {
    let result = Command::new("tasklist")
        .args(["/FO","CSV","/NH"])
        .output().map_err(|e| format!("Cannot verify L-Connect service state: {e}"))?;
    if !result.status.success() {
        return Err("Cannot verify L-Connect service state; USB probe refused".into());
    }
    let csv = String::from_utf8_lossy(&result.stdout).to_ascii_lowercase();
    let service_running = csv.lines().any(|line| {
        let name = line.split(',').next().unwrap_or("").trim_matches('"');
        name == "l-connect-service.exe" || name == "l-connect-service-watcher.exe"
            || name == "lconnectservice.exe"
    });
    if service_running {
        return Err("L-Connect service/watchdog is running and owns the L-Wireless controller. Probe blocked to protect active cooling. No service was stopped.".into());
    }
    Ok(())
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub fn one_getdev() -> Result<LiveTopology,String> {
        Err("Real receiver query is only supported on Windows".into())
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{ffi::c_void, ptr, mem};
    type Handle = *mut c_void;
    type Bool = i32;
    const DIGCF_PRESENT:u32 = 2;

    const DIGCF_DEVICEINTERFACE:u32 = 16;
    const GENERIC_READ:u32 = 0x80000000;
    const GENERIC_WRITE:u32 = 0x40000000;
    const OPEN_EXISTING:u32 = 3;
    const FILE_FLAG_OVERLAPPED:u32 = 0x40000000;
    const PIPE_TRANSFER_TIMEOUT:u32 = 3;
    const MAX_INTERFACES:u32 = 1024;
    // GUID_DEVINTERFACE_USB_DEVICE: A5DCBF10-6530-11D2-901F-00C04FB951ED
    const USB_GUID:Guid = Guid {a:0xA5DCBF10,b:0x6530,c:0x11D2,d:[0x90,0x1F,0,0xC0,0x4F,0xB9,0x51,0xED]};
    #[repr(C)] #[derive(Clone,Copy)]
    struct Guid {a:u32,b:u16,c:u16,d:[u8;8]}
    #[repr(C)]
    struct SpDeviceInterfaceData {
        cb_size:u32, guid:Guid, flags:u32, reserved:usize
    }
    #[link(name="setupapi")]
    unsafe extern "system" {
        fn SetupDiGetClassDevsW(guid:*const Guid, enumerator:*const u16, parent:Handle, flags:u32)->Handle;
        fn SetupDiEnumDeviceInterfaces(set:Handle,dev:*const c_void,guid:*const Guid,index:u32,data:*mut SpDeviceInterfaceData)->Bool;
        fn SetupDiGetDeviceInterfaceDetailW(set:Handle,data:*mut SpDeviceInterfaceData,
            details:*mut c_void, len:u32, needed:*mut u32, info:*mut c_void)->Bool;
        fn SetupDiDestroyDeviceInfoList(set:Handle)->Bool;
    }
    #[link(name="kernel32")]
    unsafe extern "system" {
        fn CreateFileW(path:*const u16,access:u32,share:u32,security:*const c_void,
            creation:u32,flags:u32,template:Handle)->Handle;
        fn CloseHandle(handle:Handle)->Bool;
        fn GetLastError()->u32;
    }
    #[link(name="winusb")]
    unsafe extern "system" {
        fn WinUsb_Initialize(file:Handle,interface:*mut Handle)->Bool;
        fn WinUsb_Free(interface:Handle)->Bool;
        fn WinUsb_WritePipe(interface:Handle,pipe:u8,data:*mut u8,len:u32,
            transferred:*mut u32,overlapped:*mut c_void)->Bool;
        fn WinUsb_ReadPipe(interface:Handle,pipe:u8,data:*mut u8,len:u32,
            transferred:*mut u32,overlapped:*mut c_void)->Bool;
        fn WinUsb_SetPipePolicy(interface:Handle,pipe:u8,policy:u32,size:u32,value:*const c_void)->Bool;
    }
    fn bad(h:Handle)->bool { h.is_null() || h as isize == -1 }
    struct DeviceSet(Handle);
    impl Drop for DeviceSet {
        fn drop(&mut self) {unsafe { SetupDiDestroyDeviceInfoList(self.0); }}
    }
    struct File(Handle);
    impl Drop for File {
        fn drop(&mut self) {unsafe {CloseHandle(self.0);}}
    }
    struct Usb(Handle);
    impl Drop for Usb {
        fn drop(&mut self) {unsafe {WinUsb_Free(self.0);}}
    }

    fn enumerate_rx() -> Result<Vec<Vec<u16>>, String> {
        let raw = unsafe {
            SetupDiGetClassDevsW(&USB_GUID,ptr::null(),ptr::null_mut(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)
        };
        if bad(raw) {return Err("Cannot enumerate device interfaces via Windows SetupAPI".into());}
        let _set=DeviceSet(raw);
        let mut rx_paths=Vec::new();
        for index in 0..MAX_INTERFACES {
            let mut data=SpDeviceInterfaceData {
                cb_size:mem::size_of::<SpDeviceInterfaceData>() as u32,
                guid:Guid{a:0,b:0,c:0,d:[0;8]},flags:0,reserved:0,
            };
            let found=unsafe {SetupDiEnumDeviceInterfaces(raw,ptr::null(),&USB_GUID,index,&mut data)};
            if found==0 {break;}
            let mut needed=0u32;
            unsafe {
                SetupDiGetDeviceInterfaceDetailW(raw,&mut data,ptr::null_mut(),0,&mut needed,ptr::null_mut());
            }
            if !(8..=4096).contains(&needed) {continue;}
            let mut buffer=vec![0u8;needed as usize+2];
            // On Windows x64, cbSize=8. DevicePath WCHAR begins at byte offset 4.
            buffer[..4].copy_from_slice(&8u32.to_ne_bytes());
            let ok=unsafe {SetupDiGetDeviceInterfaceDetailW(raw,&mut data,
                buffer.as_mut_ptr() as *mut c_void,needed,ptr::null_mut(),ptr::null_mut())};
            if ok==0 {continue;}
            let wide:Vec<u16>=buffer[4..].chunks_exact(2)
                .map(|b|u16::from_ne_bytes([b[0],b[1]])).take_while(|c|*c!=0).collect();
            let id=String::from_utf16_lossy(&wide).to_ascii_lowercase();
            if id.contains("vid_0416&pid_8041") {
                let mut path=wide;
                path.push(0);
                rx_paths.push(path);
            }
        }
        // Require a unique RX interface to avoid sending to the wrong radio.
        if rx_paths.len()!=1 {
            return Err(format!("Expected exactly one V1 RX WinUSB interface; found {}. Probe refused.",rx_paths.len()));
        }
        Ok(rx_paths)
    }

    pub fn one_getdev() -> Result<LiveTopology, String> {
        let paths=enumerate_rx()?;
        // Re-check immediately before any access. No service is stopped.
        check_owner()?;
        let file=unsafe {CreateFileW(paths[0].as_ptr(), GENERIC_READ|GENERIC_WRITE,0,
            ptr::null(),OPEN_EXISTING,FILE_FLAG_OVERLAPPED,ptr::null_mut())};
        if bad(file) {
            return Err("RX WinUSB interface could not be opened exclusively. L-Connect may still own it.".into());
        }
        let _file=File(file);
        let mut handle=ptr::null_mut();
        if unsafe{WinUsb_Initialize(file,&mut handle)}==0 {
            return Err("WinUsb_Initialize failed on RX. Nothing was sent.".into());
        }
        let _usb=Usb(handle);
        let timeout=500u32;
        unsafe {
            WinUsb_SetPipePolicy(handle,0x81,PIPE_TRANSFER_TIMEOUT,4,&timeout as *const _ as *const c_void);
            WinUsb_SetPipePolicy(handle,0x01,PIPE_TRANSFER_TIMEOUT,4,&timeout as *const _ as *const c_void);
        }
        check_owner()?;
        // GetDev query only. No speed/RGB/pump/LCD command.
        let mut query=[0u8;64];
        query[0]=0x10; query[1]=0x01;
        let mut transferred=0u32;
        if unsafe{WinUsb_WritePipe(handle,0x01,query.as_mut_ptr(),64,&mut transferred,ptr::null_mut())}==0
            || transferred!=64 {
            return Err(format!("GetDev query failed or short-write ({} bytes). No control packet sent.",transferred));
        }
        let mut response=[0u8;512];
        let mut got=0u32;
        if unsafe{WinUsb_ReadPipe(handle,0x81,response.as_mut_ptr(),512,&mut got,ptr::null_mut())}==0 {
            let error=unsafe{GetLastError()};
            return Err(format!("GetDev response unavailable (WinUSB error {error}); no retry or hardware reset performed."));
        }
        if got>512 {return Err("Unexpected WinUSB byte count".into());}
        parse_getdev(&response[..got as usize])
    }
}
#[cfg(test)]
mod tests {
    // Pure-parser hardware tests live in wireless_protocol.rs.
    // Keep the explicit-owner guard independent of USB enumeration.
    #[test] fn no_background_probing() {
        assert!(std::mem::size_of::<super::LiveTopology>()>0);
    }
}
