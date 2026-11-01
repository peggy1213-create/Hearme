import AVFoundation
import CoreAudio
import Foundation

/// Prefix for every aggregate device and tap this helper creates, so orphans can be
/// recognised and reaped (see `Cleanup`).
public let hearmeDevicePrefix = "com.hearme.tapcapture"

/// Captures the meeting track: a mono mixdown process tap over the chosen app's
/// processes, read through a **private**, **unmuted** aggregate device so the user
/// still hears the meeting and nothing shows up in Audio MIDI Setup. Audio is handed
/// to `sink` on a Core Audio IO thread.
public final class MeetingTap {
    public typealias Sink = (_ samples: [Float], _ hostTimeNs: UInt64, _ sampleRate: UInt32, _ channels: UInt8) -> Void

    private let sink: Sink
    private var processIDs: [AudioObjectID]

    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var ioProcID: AudioDeviceIOProcID?
    private var tapUID: String?

    public private(set) var sampleRate: UInt32 = 0
    public private(set) var channels: UInt8 = 1

    public init(processIDs: [AudioObjectID], sink: @escaping Sink) {
        self.processIDs = processIDs
        self.sink = sink
    }

    public enum TapError: Error {
        case createTapFailed(OSStatus)
        case tapUIDUnavailable
        case noDefaultOutput
        case createAggregateFailed(OSStatus)
        case ioProcFailed(OSStatus)
        case startFailed(OSStatus)
        case noProcesses
    }

    /// Create the tap + aggregate device and start the IO proc.
    public func start() throws {
        guard !processIDs.isEmpty else { throw TapError.noProcesses }
        try createTap()
        try buildAggregateAndStart()
    }

    /// Stop and fully tear down. Safe to call more than once.
    public func stop() {
        if let ioProcID {
            AudioDeviceStop(aggregateID, ioProcID)
            AudioDeviceDestroyIOProcID(aggregateID, ioProcID)
            self.ioProcID = nil
        }
        if aggregateID != kAudioObjectUnknown {
            AudioHardwareDestroyAggregateDevice(aggregateID)
            aggregateID = AudioObjectID(kAudioObjectUnknown)
        }
        if tapID != kAudioObjectUnknown {
            AudioHardwareDestroyProcessTap(tapID)
            tapID = AudioObjectID(kAudioObjectUnknown)
        }
        tapUID = nil
    }

    /// The default output device changed (e.g. headphones). The tap is independent of
    /// the output device, so keep it and rebuild only the aggregate around the new
    /// default output.
    public func restartForOutputChange() throws {
        if let ioProcID {
            AudioDeviceStop(aggregateID, ioProcID)
            AudioDeviceDestroyIOProcID(aggregateID, ioProcID)
            self.ioProcID = nil
        }
        if aggregateID != kAudioObjectUnknown {
            AudioHardwareDestroyAggregateDevice(aggregateID)
            aggregateID = AudioObjectID(kAudioObjectUnknown)
        }
        try buildAggregateAndStart()
    }

    /// The meeting app restarted; refresh the tapped process set and rebuild the tap.
    public func updateProcesses(_ ids: [AudioObjectID]) throws {
        processIDs = ids
        stop()
        try start()
    }

    // MARK: - Internals

    private func createTap() throws {
        let description = CATapDescription(monoMixdownOfProcesses: processIDs)
        description.name = "\(hearmeDevicePrefix).tap"
        description.uuid = UUID()
        description.muteBehavior = .unmuted  // user still hears the meeting
        description.isPrivate = true
        description.isMixdown = true
        description.isMono = true

        var newTap = AudioObjectID(kAudioObjectUnknown)
        let status = AudioHardwareCreateProcessTap(description, &newTap)
        guard status == noErr, newTap != kAudioObjectUnknown else {
            throw TapError.createTapFailed(status)
        }
        tapID = newTap

        // Read the tap's UID (string) to reference it from the aggregate device.
        tapUID = HAL.string(tapID, kAudioTapPropertyUID) ?? description.uuid.uuidString

        // Read the tap's stream format for sampleRate / channels.
        if let asbd = tapFormat() {
            sampleRate = UInt32(asbd.mSampleRate)
            channels = UInt8(max(1, asbd.mChannelsPerFrame))
        }
    }

    private func tapFormat() -> AudioStreamBasicDescription? {
        var addr = HAL.address(kAudioTapPropertyFormat)
        var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        var asbd = AudioStreamBasicDescription()
        let status = AudioObjectGetPropertyData(tapID, &addr, 0, nil, &size, &asbd)
        return status == noErr ? asbd : nil
    }

    private func buildAggregateAndStart() throws {
        guard let tapUID else { throw TapError.tapUIDUnavailable }
        guard let outputUID = Self.defaultOutputDeviceUID() else { throw TapError.noDefaultOutput }

        let aggregateUID = "\(hearmeDevicePrefix).\(UUID().uuidString)"
        let description: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Hearme Tap Capture",
            kAudioAggregateDeviceUIDKey: aggregateUID,
            kAudioAggregateDeviceMainSubDeviceKey: outputUID,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [
                [kAudioSubDeviceUIDKey: outputUID],
            ],
            kAudioAggregateDeviceTapListKey: [
                [
                    kAudioSubTapDriftCompensationKey: true,
                    kAudioSubTapUIDKey: tapUID,
                ],
            ],
        ]

        var newAgg = AudioObjectID(kAudioObjectUnknown)
        let status = AudioHardwareCreateAggregateDevice(description as CFDictionary, &newAgg)
        guard status == noErr, newAgg != kAudioObjectUnknown else {
            throw TapError.createAggregateFailed(status)
        }
        aggregateID = newAgg

        // Install the IO proc. This (first IO start) is when macOS prompts for the
        // system-audio-recording permission.
        var procID: AudioDeviceIOProcID?
        let queue = DispatchQueue(label: "\(hearmeDevicePrefix).meeting.io", qos: .userInitiated)
        let ioStatus = AudioDeviceCreateIOProcIDWithBlock(&procID, aggregateID, queue) {
            [weak self] _, inInputData, inInputTime, _, _ in
            self?.handleIO(inInputData: inInputData, inInputTime: inInputTime)
        }
        guard ioStatus == noErr, let procID else { throw TapError.ioProcFailed(ioStatus) }
        ioProcID = procID

        let startStatus = AudioDeviceStart(aggregateID, procID)
        guard startStatus == noErr else { throw TapError.startFailed(startStatus) }
    }

    private func handleIO(
        inInputData: UnsafePointer<AudioBufferList>,
        inInputTime: UnsafePointer<AudioTimeStamp>
    ) {
        let bufferList = UnsafeMutableAudioBufferListPointer(
            UnsafeMutablePointer(mutating: inInputData))
        guard let buffer = bufferList.first, let data = buffer.mData else { return }

        let count = Int(buffer.mDataByteSize) / MemoryLayout<Float>.size
        guard count > 0 else { return }
        let samples = Array(UnsafeBufferPointer(
            start: data.assumingMemoryBound(to: Float.self), count: count))

        let ns = Clock.hostTimeToNs(inInputTime.pointee.mHostTime)
        let ch = UInt8(max(1, buffer.mNumberChannels))
        sink(samples, ns, sampleRate, ch)
    }

    static func defaultOutputDeviceUID() -> String? {
        guard let deviceID = HAL.uint32(AudioObjectID(kAudioObjectSystemObject),
                                        kAudioHardwarePropertyDefaultOutputDevice),
              deviceID != 0
        else { return nil }
        return HAL.string(AudioObjectID(deviceID), kAudioDevicePropertyDeviceUID)
    }
}
