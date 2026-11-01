import AVFoundation
import CoreAudio
import Foundation

/// Captures the microphone track with `AVAudioEngine`. Emits interleaved `float32` at
/// the input's native rate and channel count, stamped with the shared host clock so
/// it aligns with the meeting track. Requires `NSMicrophoneUsageDescription`.
public final class MicCapture {
    public typealias Sink = (_ samples: [Float], _ hostTimeNs: UInt64, _ sampleRate: UInt32, _ channels: UInt8) -> Void

    private let sink: Sink
    private let micDeviceUID: String?
    private let engine = AVAudioEngine()
    private var running = false
    private var configObserver: NSObjectProtocol?

    public init(micDeviceUID: String?, sink: @escaping Sink) {
        self.micDeviceUID = micDeviceUID
        self.sink = sink
    }

    public enum MicError: Error {
        case engineStartFailed(Error)
        case noInputFormat
    }

    public func start() throws {
        if let micDeviceUID, let deviceID = Self.deviceID(forUID: micDeviceUID) {
            setInputDevice(deviceID)
        }

        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { throw MicError.noInputFormat }

        input.installTap(onBus: 0, bufferSize: 4096, format: format) { [weak self] buffer, when in
            self?.handle(buffer: buffer, when: when)
        }

        engine.prepare()
        do {
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            throw MicError.engineStartFailed(error)
        }
        running = true

        // Rebuild on route/device changes (e.g. unplugging headphones changes the
        // default input). Core Audio sends a configuration-change notification.
        configObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
        ) { [weak self] _ in
            self?.handleConfigChange()
        }
    }

    public func stop() {
        if let configObserver {
            NotificationCenter.default.removeObserver(configObserver)
            self.configObserver = nil
        }
        guard running else { return }
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        running = false
    }

    // MARK: - Internals

    private func handle(buffer: AVAudioPCMBuffer, when: AVAudioTime) {
        guard let channelData = buffer.floatChannelData else { return }
        let channels = Int(buffer.format.channelCount)
        let frames = Int(buffer.frameLength)
        guard frames > 0, channels > 0 else { return }

        // Interleave channel-major AVAudioEngine buffers into frame-major float32.
        var interleaved = [Float](repeating: 0, count: frames * channels)
        for ch in 0..<channels {
            let src = channelData[ch]
            for f in 0..<frames {
                interleaved[f * channels + ch] = src[f]
            }
        }

        let ns = when.isHostTimeValid ? Clock.hostTimeToNs(when.hostTime) : Clock.nowNs()
        sink(interleaved, ns, UInt32(buffer.format.sampleRate), UInt8(channels))
    }

    private func handleConfigChange() {
        guard running else { return }
        engine.inputNode.removeTap(onBus: 0)
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0 else { return }
        input.installTap(onBus: 0, bufferSize: 4096, format: format) { [weak self] buffer, when in
            self?.handle(buffer: buffer, when: when)
        }
        if !engine.isRunning {
            try? engine.start()
        }
    }

    private func setInputDevice(_ deviceID: AudioDeviceID) {
        guard let audioUnit = engine.inputNode.audioUnit else { return }
        var id = deviceID
        AudioUnitSetProperty(
            audioUnit,
            kAudioOutputUnitProperty_CurrentDevice,
            kAudioUnitScope_Global,
            0,
            &id,
            UInt32(MemoryLayout<AudioDeviceID>.size)
        )
    }

    /// Resolve a Core Audio device UID to its `AudioDeviceID`.
    static func deviceID(forUID uid: String) -> AudioDeviceID? {
        let devices = HAL.objectIDs(AudioObjectID(kAudioObjectSystemObject),
                                    kAudioHardwarePropertyDevices)
        for device in devices {
            if HAL.string(device, kAudioDevicePropertyDeviceUID) == uid {
                return device
            }
        }
        return nil
    }
}
