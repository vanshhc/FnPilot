use fnpilot_core::{Action, Result, runner::Outcome};
use std::{ffi::c_void, time::Instant};
#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}
const fn code(value: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*value)
}
#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectGetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectSetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> i32;
}
fn read<T: Default>(object: u32, address: &Address) -> Result<T> {
    let mut value = T::default();
    let mut size = std::mem::size_of::<T>() as u32;
    let error = unsafe {
        AudioObjectGetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            &mut size,
            (&mut value as *mut T).cast(),
        )
    };
    if error == 0 && size == std::mem::size_of::<T>() as u32 {
        Ok(value)
    } else {
        Err("This output device does not expose that system audio control.".into())
    }
}
fn write<T>(
    object: u32,
    address: &Address,
    value: &T,
    deadline: Instant,
    epoch: u64,
) -> Result<()> {
    super::check(deadline, epoch)?;
    if unsafe {
        AudioObjectSetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            std::mem::size_of::<T>() as u32,
            (value as *const T).cast(),
        )
    } == 0
    {
        Ok(())
    } else {
        Err("The output device rejected the audio setting.".into())
    }
}
pub fn audio(action: &Action, deadline: Instant, epoch: u64) -> Result<Outcome> {
    let device: u32 = read(
        1,
        &Address {
            selector: code(b"dOut"),
            scope: code(b"glob"),
            element: 0,
        },
    )?;
    if device == 0 {
        return Err("No audio output device is available.".into());
    }
    let mut property = Address {
        selector: code(b"volm"),
        scope: code(b"outp"),
        element: 0,
    };
    if ["mute", "unmute"].contains(&action.kind.as_str()) {
        property.selector = code(b"mute");
        let wanted = u32::from(action.kind == "mute");
        write(device, &property, &wanted, deadline, epoch)?;
        return Ok(Outcome {
            message: "Mute updated".into(),
            verified: read::<u32>(device, &property)? == wanted,
        });
    }
    let before: f32 = read(device, &property)?;
    let wanted = action
        .value
        .map(|n| n as f32 / 100.)
        .unwrap_or(
            before
                + if action.kind == "volume_up" {
                    0.1
                } else {
                    -0.1
                },
        )
        .clamp(0., 1.);
    write(device, &property, &wanted, deadline, epoch)?;
    Ok(Outcome {
        message: format!("Volume {}%", (wanted * 100.) as u32),
        verified: (read::<f32>(device, &property)? - wanted).abs() < 0.02,
    })
}
pub fn brightness(action: &Action, deadline: Instant, epoch: u64) -> Result<Outcome> {
    type Get = unsafe extern "C" fn(u32, *mut f32) -> i32;
    type Set = unsafe extern "C" fn(u32, f32) -> i32;
    struct Library(*mut c_void);
    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                if !self.0.is_null() {
                    libc::dlclose(self.0);
                }
            }
        }
    }
    unsafe {
        let library = Library(libc::dlopen(
            c"/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices".as_ptr(),
            libc::RTLD_LAZY,
        ));
        if library.0.is_null() {
            return Err("Built-in brightness is unavailable on this macOS version.".into());
        }
        let get = libc::dlsym(library.0, c"DisplayServicesGetBrightness".as_ptr());
        let set = libc::dlsym(library.0, c"DisplayServicesSetBrightness".as_ptr());
        if get.is_null() || set.is_null() {
            return Err("Built-in brightness is unavailable on this macOS version.".into());
        }
        let get: Get = std::mem::transmute(get);
        let set: Set = std::mem::transmute(set);
        let display = core_graphics::display::CGDisplay::active_displays()
            .map_err(|_| "Displays are unavailable.")?
            .into_iter()
            .find(|id| core_graphics::display::CGDisplay::new(*id).is_builtin())
            .ok_or("External-monitor brightness is unsupported; no built-in display was found.")?;
        let mut before = 0.;
        if get(display, &mut before) != 0 {
            return Err("Could not read built-in brightness.".into());
        }
        let wanted = action
            .value
            .map(|n| n as f32 / 100.)
            .unwrap_or(
                before
                    + if action.kind == "brightness_up" {
                        0.1
                    } else {
                        -0.1
                    },
            )
            .clamp(0., 1.);
        super::check(deadline, epoch)?;
        if set(display, wanted) != 0 {
            return Err("Could not change built-in brightness.".into());
        }
        let mut after = 0.;
        Ok(Outcome {
            message: format!("Brightness {}%", (wanted * 100.) as u32),
            verified: get(display, &mut after) == 0 && (wanted - after).abs() < 0.03,
        })
    }
}
