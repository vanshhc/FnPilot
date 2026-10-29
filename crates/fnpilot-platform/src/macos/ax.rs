//! Owned Accessibility values. Native handles never leave the serialized adapter worker.
use core_foundation::{
    array::CFArray,
    base::{CFType, CFTypeRef, TCFType},
    boolean::CFBoolean,
    dictionary::CFDictionary,
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use fnpilot_core::Result;
use std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementCopyElementAtPosition(
        element: CFTypeRef,
        x: f32,
        y: f32,
        result: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementGetTypeID() -> usize;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        key: CFStringRef,
        result: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(element: CFTypeRef, key: CFStringRef, value: CFTypeRef) -> i32;
    fn AXUIElementPerformAction(element: CFTypeRef, action: CFStringRef) -> i32;
    fn AXUIElementGetPid(element: CFTypeRef, pid: *mut i32) -> i32;
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> i32;
    fn AXUIElementIsAttributeSettable(
        element: CFTypeRef,
        key: CFStringRef,
        settable: *mut u8,
    ) -> i32;
    fn AXValueCreate(kind: u32, value: *const c_void) -> CFTypeRef;
    fn AXValueGetTypeID() -> usize;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, result: *mut c_void) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncated_and_malformed_native_child_arrays_are_never_complete() {
        // Creating an AX application reference does not query or mutate the desktop.
        let app = Element::application(std::process::id());
        let values = vec![app.0; 701];
        let array = CFArray::from_CFTypes(&values);
        let (children, complete) = Element::bounded_array(array.as_CFType());
        assert_eq!(children.len(), 700);
        assert!(!complete);
        let valid = CFArray::from_CFTypes(&values[..2]);
        assert_eq!(Element::bounded_array(valid.as_CFType()).0.len(), 2);
        assert!(Element::bounded_array(valid.as_CFType()).1);
        let malformed = CFArray::from_CFTypes(&[CFString::new("not an AX element")]);
        assert!(!Element::bounded_array(malformed.as_CFType()).1);
        assert!(!Element::bounded_array(CFBoolean::true_value().as_CFType()).1);
    }
}
pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}
pub fn request_trust() -> bool {
    let options = CFDictionary::from_CFType_pairs(&[(
        CFString::new("AXTrustedCheckOptionPrompt"),
        CFBoolean::true_value(),
    )]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_CFTypeRef()) }
}
#[derive(Clone, Debug)]
pub struct Element(CFType);
impl PartialEq for Element {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Element {
    pub fn at_position(x: f64, y: f64) -> Option<Self> {
        unsafe {
            let root = CFType::wrap_under_create_rule(AXUIElementCreateSystemWide());
            let mut result = std::ptr::null();
            if AXUIElementCopyElementAtPosition(
                root.as_CFTypeRef(),
                x as f32,
                y as f32,
                &mut result,
            ) != 0
                || result.is_null()
            {
                return None;
            }
            Self::from_value(CFType::wrap_under_create_rule(result))
        }
    }
    pub fn application(pid: u32) -> Self {
        unsafe {
            Self(CFType::wrap_under_create_rule(
                AXUIElementCreateApplication(pid as i32),
            ))
        }
    }
    fn from_value(value: CFType) -> Option<Self> {
        (value.type_of() == unsafe { AXUIElementGetTypeID() }).then_some(Self(value))
    }
    pub fn hash(&self) -> u64 {
        unsafe { core_foundation::base::CFHash(self.0.as_CFTypeRef()) as u64 }
    }
    pub fn timeout(&self) {
        unsafe {
            AXUIElementSetMessagingTimeout(self.0.as_CFTypeRef(), 0.15);
        }
    }
    pub fn attribute(&self, key: &str) -> Option<CFType> {
        self.copy_attribute(key).ok()
    }
    fn copy_attribute(&self, key: &str) -> std::result::Result<CFType, i32> {
        self.timeout();
        let mut value = std::ptr::null();
        let status = unsafe {
            AXUIElementCopyAttributeValue(
                self.0.as_CFTypeRef(),
                CFString::new(key).as_concrete_TypeRef(),
                &mut value,
            )
        };
        if status == 0 && !value.is_null() {
            Ok(unsafe { CFType::wrap_under_create_rule(value) })
        } else {
            Err(status)
        }
    }
    pub fn string(&self, key: &str) -> String {
        self.attribute(key)
            .and_then(|v| v.downcast::<CFString>())
            .filter(|s| s.char_len() <= 65536)
            .map(|s| s.to_string())
            .unwrap_or_default()
    }
    pub fn boolean(&self, key: &str) -> Option<bool> {
        self.attribute(key).and_then(|v| {
            v.downcast::<CFBoolean>().map(bool::from).or_else(|| {
                v.downcast::<CFNumber>()
                    .and_then(|n| n.to_i64())
                    .map(|n| n != 0)
            })
        })
    }
    pub fn number(&self, key: &str) -> Option<String> {
        self.attribute(key)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_f64())
            .map(|n| n.to_string())
    }
    pub fn element(&self, key: &str) -> Option<Self> {
        self.attribute(key).and_then(Self::from_value)
    }
    pub fn elements(&self, key: &str) -> Vec<Self> {
        self.elements_bounded(key).0
    }
    pub fn elements_bounded(&self, key: &str) -> (Vec<Self>, bool) {
        match self.copy_attribute(key) {
            Ok(value) => Self::bounded_array(value),
            // Unsupported/no-value children are leaves; provider errors are incomplete scans.
            Err(-25205 | -25212) => (vec![], true),
            Err(_) => (vec![], false),
        }
    }
    pub fn visible_children(&self) -> (Vec<Self>, bool, Option<i32>) {
        match self.copy_attribute("AXVisibleChildren") {
            Ok(value) => {
                let (children, complete) = Self::bounded_array(value);
                (children, complete, None)
            }
            Err(-25205 | -25212) => {
                let (children, complete) = self.elements_bounded("AXChildren");
                (children, complete, None)
            }
            Err(code) => (vec![], false, Some(code)),
        }
    }
    fn bounded_array(value: CFType) -> (Vec<Self>, bool) {
        let Some(array) = value.downcast::<CFArray>() else {
            return (vec![], false);
        };
        let count = array.len();
        let mut complete = count <= 700;
        let values = (0..count.min(700))
            .filter_map(|index| array.get(index))
            .filter_map(|pointer| {
                let element = Self::from_value(unsafe { CFType::wrap_under_get_rule(*pointer) });
                complete &= element.is_some();
                element
            })
            .collect();
        (values, complete)
    }
    pub fn pid(&self) -> Option<u32> {
        let mut pid = 0;
        (unsafe { AXUIElementGetPid(self.0.as_CFTypeRef(), &mut pid) } == 0).then_some(pid as u32)
    }
    pub fn editable(&self) -> bool {
        self.timeout();
        let mut writable = 0;
        unsafe {
            AXUIElementIsAttributeSettable(
                self.0.as_CFTypeRef(),
                CFString::new("AXValue").as_concrete_TypeRef(),
                &mut writable,
            ) == 0
                && writable != 0
        }
    }
    fn set_value(&self, key: &str, value: CFTypeRef) -> Result<()> {
        self.timeout();
        let error = unsafe {
            AXUIElementSetAttributeValue(
                self.0.as_CFTypeRef(),
                CFString::new(key).as_concrete_TypeRef(),
                value,
            )
        };
        if error == 0 {
            Ok(())
        } else {
            Err(format!("The app rejected {key} (Accessibility {error})."))
        }
    }
    pub fn set_text(&self, key: &str, value: &str) -> Result<()> {
        self.set_value(key, CFString::new(value).as_CFTypeRef())
    }
    pub fn set_bool(&self, key: &str, value: bool) -> Result<()> {
        self.set_value(key, CFBoolean::from(value).as_CFTypeRef())
    }
    pub fn perform(&self, action: &str) -> Result<()> {
        self.timeout();
        if unsafe {
            AXUIElementPerformAction(
                self.0.as_CFTypeRef(),
                CFString::new(action).as_concrete_TypeRef(),
            )
        } == 0
        {
            Ok(())
        } else {
            Err("The app rejected this accessible action.".into())
        }
    }
    pub fn frame(&self) -> Option<CGRect> {
        let (position, size) = (self.attribute("AXPosition")?, self.attribute("AXSize")?);
        if position.type_of() != unsafe { AXValueGetTypeID() }
            || size.type_of() != unsafe { AXValueGetTypeID() }
        {
            return None;
        }
        let (mut p, mut s) = (CGPoint::new(0., 0.), CGSize::new(0., 0.));
        let ok = unsafe {
            AXValueGetValue(position.as_CFTypeRef(), 1, (&mut p as *mut CGPoint).cast())
                && AXValueGetValue(size.as_CFTypeRef(), 2, (&mut s as *mut CGSize).cast())
        };
        ok.then_some(CGRect::new(&p, &s))
    }
    pub fn set_size(&self, size: CGSize) -> Result<()> {
        self.set_geometry("AXSize", 2, (&size as *const CGSize).cast())
    }
    pub fn set_position(&self, position: CGPoint) -> Result<()> {
        self.set_geometry("AXPosition", 1, (&position as *const CGPoint).cast())
    }
    fn set_geometry(&self, key: &str, kind: u32, pointer: *const c_void) -> Result<()> {
        let raw = unsafe { AXValueCreate(kind, pointer) };
        if raw.is_null() {
            return Err("Could not create window geometry.".into());
        }
        let value = unsafe { CFType::wrap_under_create_rule(raw) };
        self.set_value(key, value.as_CFTypeRef())
    }
}
