import CoreAudio
import Foundation

/// `record` debug subcommand: capture a bundle's output plus the mic for a fixed time
/// and write `mic.wav` + `meeting.wav` so a human can listen and confirm the two
/// tracks are clean and aligned.
public enum Recorder {
    public struct Options {
        public var bundlePrefix: String
        public var seconds: Double
        public var outDir: URL
    }

    public static func run(_ options: Options) -> Int32 {
        let all = ProcessList.current()
        let procs = all.filter { $0.bundleID.lowercased().hasPrefix(options.bundlePrefix.lowercased()) }
        guard !procs.isEmpty else {
            logStderr("record: no running process matches bundle prefix \"\(options.bundlePrefix)\"")
            logStderr("record: running audio processes: " +
                all.map { $0.bundleID }.sorted().joined(separator: ", "))
            return 2
        }

        try? FileManager.default.createDirectory(at: options.outDir, withIntermediateDirectories: true)
        let meetingURL = options.outDir.appendingPathComponent("meeting.wav")
        let micURL = options.outDir.appendingPathComponent("mic.wav")

        // Writers are created on the first frame, when we know each track's rate.
        var meetingWriter: WavWriter?
        var micWriter: WavWriter?
        let writerLock = NSLock()

        let tap = MeetingTap(processIDs: procs.map { $0.objectID }) { samples, _, rate, ch in
            writerLock.lock()
            if meetingWriter == nil {
                meetingWriter = try? WavWriter(url: meetingURL, sampleRate: rate, channels: UInt16(ch))
            }
            let w = meetingWriter
            writerLock.unlock()
            w?.append(samples)
        }

        let mic = MicCapture(micDeviceUID: nil) { samples, _, rate, ch in
            writerLock.lock()
            if micWriter == nil {
                micWriter = try? WavWriter(url: micURL, sampleRate: rate, channels: UInt16(ch))
            }
            let w = micWriter
            writerLock.unlock()
            w?.append(samples)
        }

        do {
            try tap.start()
            try mic.start()
        } catch {
            logStderr("record: failed to start capture: \(error)")
            tap.stop()
            mic.stop()
            return 1
        }

        logStderr("record: capturing \(procs.count) process(es) for \(options.seconds)s " +
            "to \(options.outDir.path) … (grant audio permission if prompted)")
        RunLoop.main.run(until: Date().addingTimeInterval(options.seconds))

        tap.stop()
        mic.stop()
        writerLock.lock()
        meetingWriter?.close()
        micWriter?.close()
        let wroteMeeting = meetingWriter != nil
        let wroteMic = micWriter != nil
        writerLock.unlock()

        logStderr("record: done. meeting.wav=\(wroteMeeting ? "ok" : "empty") " +
            "mic.wav=\(wroteMic ? "ok" : "empty")")
        if !wroteMeeting {
            logStderr("record: no meeting audio — is the app actually playing sound, " +
                "and did you grant system-audio permission?")
        }
        return 0
    }
}
