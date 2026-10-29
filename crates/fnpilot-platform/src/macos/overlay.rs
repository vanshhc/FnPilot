//! A nonactivating native panel. All AppKit object ownership stays on the main queue.
use fnpilot_core::Result;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use std::{
    cell::RefCell,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static VERSION: AtomicU64 = AtomicU64::new(0);
thread_local! {static PANEL:RefCell<Option<Retained<AnyObject>>>=const{RefCell::new(None)};}
pub fn show(message: Option<String>, deadline: Instant, epoch: u64) -> Result<()> {
    let version = VERSION.fetch_add(1, Ordering::SeqCst) + 1;
    super::main_dispatch(deadline, epoch, message.is_some(), move || {
        PANEL.with(|slot|unsafe{
            let Some(message)=message else {if let Some(panel)=slot.borrow().as_ref(){let _:()=msg_send![&**panel,orderOut:std::ptr::null::<AnyObject>()];}return Ok(());};
            if slot.borrow().is_none() {
                let allocated:objc2::rc::Allocated<AnyObject>=msg_send![class!(NSPanel),alloc];
                let panel:Retained<AnyObject>=msg_send![allocated,initWithContentRect:NSRect::new(NSPoint::new(0.,0.),NSSize::new(220.,42.)), styleMask:128usize, backing:2usize, defer:false];
                let _:()=msg_send![&*panel,setLevel:25isize];
                let _:()=msg_send![&*panel,setCollectionBehavior:257usize];
                let _:()=msg_send![&*panel,setOpaque:false];
                let clear:Retained<AnyObject>=msg_send![class!(NSColor),clearColor];
                let _:()=msg_send![&*panel,setBackgroundColor:&*clear];
                let _:()=msg_send![&*panel,setIgnoresMouseEvents:true];
                let _:()=msg_send![&*panel,setReleasedWhenClosed:false];
                *slot.borrow_mut()=Some(panel);
            }
            let borrow=slot.borrow();let panel=borrow.as_ref().unwrap();
            let screen:Option<Retained<AnyObject>>=msg_send![class!(NSScreen),mainScreen];
            let Some(screen)=screen else {return Ok(());};
            let visible:NSRect=msg_send![&*screen,visibleFrame];
            let width=if message.contains('\n'){560f64.min(visible.size.width-48.)}else{220.};
            let label:Retained<AnyObject>=msg_send![class!(NSTextField),wrappingLabelWithString:&*NSString::from_str(&message)];
            let _:()=msg_send![&*label,setAlignment:1isize];
            let size:NSSize=msg_send![&*label,sizeThatFits:NSSize::new(width-32.,f64::MAX)];
            let height=(size.height+24.).max(42.).min(visible.size.height-76.);
            let _:()=msg_send![&*label,setFrame:NSRect::new(NSPoint::new(16.,12.),NSSize::new(width-32.,height-24.))];
            let allocated:objc2::rc::Allocated<AnyObject>=msg_send![class!(NSVisualEffectView),alloc];
            let view:Retained<AnyObject>=msg_send![allocated,initWithFrame:NSRect::new(NSPoint::new(0.,0.),NSSize::new(width,height))];
            let _:()=msg_send![&*view,setMaterial:13isize];let _:()=msg_send![&*view,setWantsLayer:true];
            let layer:Option<Retained<AnyObject>>=msg_send![&*view,layer];if let Some(layer)=layer {let _:()=msg_send![&*layer,setCornerRadius:20f64];}
            let _:()=msg_send![&*view,addSubview:&*label];let _:()=msg_send![&**panel,setContentView:&*view];
            let _:()=msg_send![&**panel,setFrame:NSRect::new(NSPoint::new(visible.origin.x+(visible.size.width-width)/2.,visible.origin.y+38.),NSSize::new(width,height)),display:true];
            let _:()=msg_send![&**panel,orderFrontRegardless];
            let status=message.lines().next().unwrap_or_default();
            if ["Ready","Done","Could not finish"].contains(&status) {
                let after=dispatch2::DispatchTime::try_from(Duration::from_millis(if status=="Could not finish" || message.contains('\n'){5000}else{1200})).unwrap();
                let _=dispatch2::DispatchQueue::main().after(after,move || {
                    if VERSION.load(Ordering::SeqCst)==version && super::current_epoch()==epoch {
                        PANEL.with(|slot|if let Some(panel)=slot.borrow().as_ref(){let _:()=msg_send![&**panel,orderOut:std::ptr::null::<AnyObject>()];});
                    }
                });
            }
            Ok(())
        })
    })
}
