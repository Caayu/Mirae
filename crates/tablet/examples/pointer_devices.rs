//! Lists the pointer devices Windows knows about (is a real pen attached?).
//!
//! ```sh
//! cargo run -p tela-tablet --example pointer_devices
//! ```

#[cfg(windows)]
#[allow(unsafe_code)]
fn main() {
    use windows_sys::Win32::UI::Controls::POINTER_DEVICE_INFO;
    use windows_sys::Win32::UI::Input::Pointer::GetPointerDevices;

    let mut count = 0u32;
    // SAFETY: querying the count with a null buffer is the documented usage.
    unsafe { GetPointerDevices(&mut count, std::ptr::null_mut()) };
    let mut devices = vec![POINTER_DEVICE_INFO::default(); count as usize];
    // SAFETY: `devices` has room for `count` entries.
    if count > 0 && unsafe { GetPointerDevices(&mut count, devices.as_mut_ptr()) } == 0 {
        eprintln!("GetPointerDevices failed");
        return;
    }
    println!("{count} pointer device(s)");
    for d in &devices[..count as usize] {
        let kind = match d.pointerDeviceType {
            1 => "integrated pen",
            2 => "external pen",
            3 => "touch",
            4 => "touch pad",
            _ => "other",
        };
        let end = d.productString.iter().position(|&c| c == 0).unwrap_or(0);
        let name = String::from_utf16_lossy(&d.productString[..end]);
        println!("- {kind}: {name:?} (max contacts {})", d.maxActiveContacts);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only");
}
