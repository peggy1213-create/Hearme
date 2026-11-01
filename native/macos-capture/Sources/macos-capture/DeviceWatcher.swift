import AppKit
import CoreAudio
import Foundation

/// Watches for the environment changes the capture path must survive: the default
/// output/input device changing (headphones plugged in or out) and system sleep/wake.
/// It only reports; the session decides how to rebuild taps.
public final class DeviceWatcher {
    public var onOutputChanged: (() -> Void)?
    public var onInputChanged: (() -> Void)?
    public var onSleep: (() -> Void)?
    public var onWake: (() -> Void)?

    private let queue = DispatchQueue(label: "\(hearmeDevicePrefix).devicewatch")
    private var outputAddr = HAL.address(kAudioHardwarePropertyDefaultOutputDevice)
    private var inputAddr = HAL.address(kAudioHardwarePropertyDefaultInputDevice)
    private var outputBlock: AudioObjectPropertyListenerBlock?
    private var inputBlock: AudioObjectPropertyListenerBlock?
    private var sleepObserver: NSObjectProtocol?
    private var wakeObserver: NSObjectProtocol?

    public init() {}

    public func start() {
        let system = AudioObjectID(kAudioObjectSystemObject)

        let outBlock: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
            self?.onOutputChanged?()
        }
        outputBlock = outBlock
        AudioObjectAddPropertyListenerBlock(system, &outputAddr, queue, outBlock)

        let inBlock: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
            self?.onInputChanged?()
        }
        inputBlock = inBlock
        AudioObjectAddPropertyListenerBlock(system, &inputAddr, queue, inBlock)

        let nc = NSWorkspace.shared.notificationCenter
        sleepObserver = nc.addObserver(
            forName: NSWorkspace.willSleepNotification, object: nil, queue: nil
        ) { [weak self] _ in self?.onSleep?() }
        wakeObserver = nc.addObserver(
            forName: NSWorkspace.didWakeNotification, object: nil, queue: nil
        ) { [weak self] _ in self?.onWake?() }
    }

    public func stop() {
        let system = AudioObjectID(kAudioObjectSystemObject)
        if let outputBlock {
            AudioObjectRemovePropertyListenerBlock(system, &outputAddr, queue, outputBlock)
            self.outputBlock = nil
        }
        if let inputBlock {
            AudioObjectRemovePropertyListenerBlock(system, &inputAddr, queue, inputBlock)
            self.inputBlock = nil
        }
        let nc = NSWorkspace.shared.notificationCenter
        if let sleepObserver { nc.removeObserver(sleepObserver); self.sleepObserver = nil }
        if let wakeObserver { nc.removeObserver(wakeObserver); self.wakeObserver = nil }
    }

    deinit { stop() }
}
