import AppKit
import FnPilotCore

@MainActor
final class HotkeyMonitor {
    var onSignal: ((HotkeyState.Signal) -> Void)?
    var onInterruption: (() -> Void)?
    private var tap: CFMachPort?
    private var source: CFRunLoopSource?
    private var state = HotkeyState()
    private var observers: [NSObjectProtocol] = []
    var running: Bool { tap != nil }

    func start() throws {
        stop()
        let mask = (CGEventMask(1) << CGEventType.flagsChanged.rawValue) | (CGEventMask(1) << CGEventType.keyDown.rawValue)
        let pointer = Unmanaged.passUnretained(self).toOpaque()
        guard let tap = CGEvent.tapCreate(tap: .cgSessionEventTap, place: .headInsertEventTap, options: .listenOnly, eventsOfInterest: mask, callback: { _, type, event, context in
            guard let context else { return Unmanaged.passUnretained(event) }
            let monitor = Unmanaged<HotkeyMonitor>.fromOpaque(context).takeUnretainedValue()
            // The event tap runs on the main run loop. Only modifier/key codes are inspected; never store typed characters.
            MainActor.assumeIsolated { monitor.receive(type, event) }
            return Unmanaged.passUnretained(event)
        }, userInfo: pointer) else { throw PilotError("Enable Input Monitoring for FnPilot, then click Enable Fn key again.") }
        self.tap = tap
        let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
        self.source = source
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)
        let center = NSWorkspace.shared.notificationCenter
        for name in [NSWorkspace.willSleepNotification, NSWorkspace.screensDidSleepNotification] {
            observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.state.reset(); self?.onInterruption?() }
            })
        }
        observers.append(center.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.state.reset()
                if let tap = self?.tap { CGEvent.tapEnable(tap: tap, enable: true) }
            }
        })
    }
    func stop() {
        if let source { CFRunLoopRemoveSource(CFRunLoopGetMain(), source, .commonModes) }
        if let tap { CFMachPortInvalidate(tap) }
        tap = nil; source = nil; state.reset()
        for observer in observers { NSWorkspace.shared.notificationCenter.removeObserver(observer) }
        observers.removeAll()
    }
    private func receive(_ type: CGEventType, _ event: CGEvent) {
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            state.reset(); onInterruption?()
            if let tap { CGEvent.tapEnable(tap: tap, enable: true) }
            return
        }
        var signal: HotkeyState.Signal?
        if type == .flagsChanged {
            let others: CGEventFlags = [.maskCommand, .maskControl, .maskAlternate, .maskShift]
            signal = state.flags(fn: event.flags.contains(.maskSecondaryFn), otherModifier: !event.flags.intersection(others).isEmpty)
        } else if type == .keyDown {
            signal = state.keyDown(escape: event.getIntegerValueField(.keyboardEventKeycode) == 53)
        }
        if let signal { onSignal?(signal) }
    }
}
