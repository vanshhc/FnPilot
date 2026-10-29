use fnpilot_core::{Result, context::UIControl};
use std::{collections::HashMap, time::Instant};
use windows::Win32::{
    Foundation::HWND,
    System::{
        Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        Ole::{SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound},
    },
    UI::Accessibility::*,
};

pub struct Controls {
    pub automation: IUIAutomation,
    pub elements: HashMap<String, IUIAutomationElement>,
}
impl Controls {
    pub fn validate_cursor(
        &self,
        point: windows::Win32::Foundation::POINT,
        process: u32,
    ) -> Result<()> {
        unsafe {
            let node = self
                .automation
                .ElementFromPoint(point)
                .map_err(|_| "Cannot inspect the cursor target.")?;
            if node
                .CurrentProcessId()
                .map_err(|_| "Cannot verify cursor ownership.")? as u32
                != process
                || node
                    .CurrentIsPassword()
                    .map_err(|_| "Cannot inspect protected input.")?
                    .as_bool()
            {
                return Err("The cursor target is outside the app or is protected.".into());
            }
        }
        Ok(())
    }
    pub fn new() -> Result<Self> {
        let automation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .map_err(|_| "Windows UI Automation is unavailable.")?;
        Ok(Self {
            automation,
            elements: HashMap::new(),
        })
    }
    pub fn scan(&mut self, hwnd: HWND, deadline: Instant) -> Result<(Vec<UIControl>, bool)> {
        self.elements.clear();
        let mut controls = vec![];
        unsafe {
            let root = self
                .automation
                .ElementFromHandle(hwnd)
                .map_err(|_| "Target accessibility is unavailable.")?;
            let condition = self
                .automation
                .CreateTrueCondition()
                .map_err(|_| "Could not create accessibility scan.")?;
            let all = root
                .FindAll(TreeScope_Descendants, &condition)
                .map_err(|_| "Target accessibility scan failed.")?;
            let count = all.Length().map_err(|_| "Incomplete accessibility scan.")?;
            let mut complete = count <= 700;
            for i in 0..count.min(700) {
                if Instant::now() > deadline {
                    return Err("Target accessibility timed out.".into());
                }
                let element = all
                    .GetElement(i)
                    .map_err(|_| "A control disappeared during scanning.")?;
                if element
                    .CurrentIsPassword()
                    .map_err(|_| "Cannot determine protected control state.")?
                    .as_bool()
                    || !element
                        .CurrentIsEnabled()
                        .map_err(|_| "Cannot determine control availability.")?
                        .as_bool()
                {
                    continue;
                }
                let label = element
                    .CurrentName()
                    .map_err(|_| "Control label is unavailable.")?
                    .to_string();
                if label.is_empty() {
                    continue;
                }
                let role = match element
                    .CurrentControlType()
                    .map_err(|_| "Control type is unavailable.")?
                {
                    UIA_ButtonControlTypeId
                    | UIA_MenuItemControlTypeId
                    | UIA_CheckBoxControlTypeId
                    | UIA_RadioButtonControlTypeId
                    | UIA_HyperlinkControlTypeId => "AXButton",
                    UIA_EditControlTypeId => "AXTextField",
                    UIA_ComboBoxControlTypeId => "AXComboBox",
                    _ => continue,
                };
                if controls.len() >= 90 {
                    complete = false;
                    continue;
                }
                let id = runtime_id(&element)?;
                controls.push(UIControl {
                    id: id.clone(),
                    role: role.into(),
                    label: label.chars().take(140).collect(),
                    focused: element
                        .CurrentHasKeyboardFocus()
                        .map(|v| v.as_bool())
                        .unwrap_or(false),
                    value: if element
                        .CurrentHasKeyboardFocus()
                        .map(|v| v.as_bool())
                        .unwrap_or(false)
                        && ["AXTextField", "AXComboBox"].contains(&role)
                    {
                        element
                            .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                            .ok()
                            .and_then(|p| p.CurrentValue().ok())
                            .map(|v| v.to_string())
                            .filter(|v| v.chars().count() <= 2048)
                    } else {
                        None
                    },
                });
                self.elements.insert(id, element);
            }
            Ok((controls, complete))
        }
    }
    pub fn validate_submission(&self, id: &str, expected: &str) -> Result<()> {
        unsafe {
            let focused = self
                .automation
                .GetFocusedElement()
                .map_err(|_| "Cannot inspect focused control.")?;
            if runtime_id(&focused)? != id
                || focused
                    .CurrentIsPassword()
                    .map_err(|_| "Protected control check failed.")?
                    .as_bool()
                || !focused
                    .CurrentIsEnabled()
                    .map_err(|_| "Control availability check failed.")?
                    .as_bool()
            {
                return Err("The focused field changed or is protected.".into());
            }
            let pattern = focused
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .map_err(|_| "Cannot inspect the focused draft.")?;
            if pattern
                .CurrentValue()
                .map_err(|_| "Cannot read the focused draft.")?
                != expected
            {
                return Err("The focused draft changed. Repeat your instruction.".into());
            }
        }
        Ok(())
    }

    pub fn safe_focus(&self) -> Result<()> {
        unsafe {
            let focused = self
                .automation
                .GetFocusedElement()
                .map_err(|_| "Cannot inspect focused control.")?;
            if focused
                .CurrentIsPassword()
                .map_err(|_| "Cannot inspect protected-field status.")?
                .as_bool()
            {
                return Err("Typing into password fields is disabled.".into());
            }
        }
        Ok(())
    }
    pub fn scroll(
        &self,
        direction: &str,
        process: u32,
        deadline: Instant,
        epoch: u64,
    ) -> Result<()> {
        unsafe {
            let mut node = self
                .automation
                .GetFocusedElement()
                .map_err(|_| "Cannot inspect the focused scroll target.")?;
            let walker = self
                .automation
                .ControlViewWalker()
                .map_err(|_| "Cannot inspect scroll ancestors.")?;
            for _ in 0..20 {
                if node
                    .CurrentProcessId()
                    .map_err(|_| "Scroll target identity is unavailable.")?
                    as u32
                    != process
                {
                    return Err("The scroll target changed applications.".into());
                }
                if let Ok(pattern) =
                    node.GetCurrentPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId)
                {
                    super::check(deadline, epoch)?;
                    let (horizontal, vertical) = match direction {
                        "left" => (ScrollAmount_LargeDecrement, ScrollAmount_NoAmount),
                        "right" => (ScrollAmount_LargeIncrement, ScrollAmount_NoAmount),
                        "up" => (ScrollAmount_NoAmount, ScrollAmount_LargeDecrement),
                        _ => (ScrollAmount_NoAmount, ScrollAmount_LargeIncrement),
                    };
                    return pattern
                        .Scroll(horizontal, vertical)
                        .map_err(|_| "The selected app refused scrolling.".into());
                }
                node = walker
                    .GetParentElement(&node)
                    .map_err(|_| "No app-scoped accessible scroll pattern is available.")?;
                super::check(deadline, epoch)?;
            }
        }
        Err("No bounded app-scoped scroll target is available.".into())
    }
    pub fn invoke(
        &self,
        id: &str,
        text: Option<&str>,
        deadline: Instant,
        epoch: u64,
    ) -> Result<()> {
        let element = self
            .elements
            .get(id)
            .ok_or("The selected control is stale.")?;
        unsafe {
            if runtime_id(element)? != id
                || element
                    .CurrentIsPassword()
                    .map_err(|_| "Protected control check failed.")?
                    .as_bool()
            {
                return Err("The control changed or is protected.".into());
            }
            if let Some(text) = text {
                let pattern: IUIAutomationValuePattern = element
                    .GetCurrentPatternAs(UIA_ValuePatternId)
                    .map_err(|_| "The control has no writable Value pattern.")?;
                if pattern
                    .CurrentIsReadOnly()
                    .map_err(|_| "Cannot inspect field write access.")?
                    .as_bool()
                {
                    return Err("The field is read-only.".into());
                }
                super::check(deadline, epoch)?;
                pattern
                    .SetValue(&windows::core::BSTR::from(text))
                    .map_err(|_| "Setting the field failed.")?;
            } else {
                let pattern: IUIAutomationInvokePattern = element
                    .GetCurrentPatternAs(UIA_InvokePatternId)
                    .map_err(|_| "The control has no Invoke pattern.")?;
                super::check(deadline, epoch)?;
                pattern
                    .Invoke()
                    .map_err(|_| "Invoking the control failed.")?;
            }
        }
        Ok(())
    }
}
unsafe fn runtime_id(element: &IUIAutomationElement) -> Result<String> {
    unsafe {
        let array = element
            .GetRuntimeId()
            .map_err(|_| "Control runtime identity is unavailable.")?;
        let result = (|| {
            let start = SafeArrayGetLBound(array, 1).map_err(|_| "Invalid control identity.")?;
            let end = SafeArrayGetUBound(array, 1).map_err(|_| "Invalid control identity.")?;
            if end - start > 64 {
                return Err("Control runtime identity is oversized.".into());
            }
            let mut values = vec![];
            for i in start..=end {
                let mut value = 0i32;
                SafeArrayGetElement(array, &i, (&mut value as *mut i32).cast())
                    .map_err(|_| "Invalid control runtime identity.")?;
                values.push(value.to_string());
            }
            Ok(format!("uia:{}", values.join(".")))
        })();
        let _ = SafeArrayDestroy(array);
        result
    }
}
