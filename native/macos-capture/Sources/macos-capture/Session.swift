import CoreAudio
import Foundation

/// A tiny lock box for the one flag shared between the control queue and the
/// real-time meeting IO thread.
final class Locked<T> {
    private var value: T
    private let lock = NSLock()
    init(_ value: T) { self.value = value }
    func get() -> T { lock.lock(); defer { lock.unlock() }; return value }
    func set(_ newValue: T) { lock.lock(); value = newValue; lock.unlock() }
}

/// Protocol mode: read JSON commands on stdin, emit binary frames on stdout. All
/// capture state is mutated only on `control` so commands, the source monitor and
/// device callbacks never race; audio callbacks run on their own threads and only
/// write frames (stdout is locked) and feed their own guard.
public final class Session {
    private let out = StdoutWriter()
    private let config = KnownApps.load()
    private let control = DispatchQueue(label: "\(hearmeDevicePrefix).control")

    private var meetingTap: MeetingTap?
    private var mic: MicCapture?
    private var activeApp: KnownApp?
    private var activeProcessIDs: [AudioObjectID] = []
    private let playing = Locked<Bool>(false)

    private var producingAudio: [String: Bool] = [:]  // sourceId -> last known state
    private let watcher = DeviceWatcher()
    private var monitorTimer: DispatchSourceTimer?
    private var signalSources: [DispatchSourceSignal] = []

    public init() {}

    public func run() {
        Cleanup.reapOrphanAggregates()

        signalSources = installTerminationHandlers(queue: .main) { [weak self] in
            self?.teardown()
        }

        watcher.onOutputChanged = { [weak self] in self?.control.async { self?.handleOutputChange() } }
        watcher.onInputChanged = { [weak self] in self?.control.async { self?.emit(Event.deviceChanged(kind: "input")) } }
        watcher.onWake = { [weak self] in self?.control.async { self?.handleWake() } }
        watcher.start()

        startMonitor()
        startStdinReader()

        RunLoop.main.run()
    }

    // MARK: - stdin

    private func startStdinReader() {
        Thread.detachNewThread { [weak self] in
            while let line = readLine(strippingNewline: true) {
                guard let self else { return }
                self.control.async { self.dispatch(line: line) }
            }
            // EOF: the core closed the pipe. Shut down cleanly.
            self?.control.async { self?.teardown(); exit(0) }
        }
    }

    private func dispatch(line: String) {
        let command: Command?
        do {
            command = try parseCommand(line: line)
        } catch {
            emit(Event.error(code: "bad_request", message: "unparseable command"))
            return
        }
        guard let command else { return }  // blank line
        switch command.cmd {
        case "list_sources":
            emit(Event.sources(ProcessList.sources(config: config, processes: ProcessList.current())))
        case "start":
            guard let sourceId = command.sourceId else {
                emit(Event.error(code: "bad_request", message: "start requires sourceId"))
                return
            }
            startCapture(sourceId: sourceId, micDeviceUID: command.micDeviceId)
        case "stop":
            stopCapture(emitStopped: true)
        case "ping":
            emit(Event.pong())
        default:
            emit(Event.error(code: "bad_request", message: "unknown command"))
        }
    }

    // MARK: - capture lifecycle (control queue only)

    private func startCapture(sourceId: String, micDeviceUID: String?) {
        guard let app = config.apps.first(where: { $0.id == sourceId }) else {
            emit(Event.error(code: "bad_request", message: "unknown sourceId"))
            return
        }
        stopCapture(emitStopped: false)

        let all = ProcessList.current()
        let procs = ProcessList.processes(for: app, in: all)
        guard !procs.isEmpty else {
            emit(Event.error(code: "bad_request", message: "source not running"))
            return
        }

        let guardRef = SilenceGuard(track: .meeting) { [weak self] track in
            self?.emit(Event.permissionSuspectedMissing(track: track), track: track)
        }
        let playingBox = playing
        let writer = out

        let tap = MeetingTap(processIDs: procs.map { $0.objectID }) { samples, ns, rate, ch in
            writer.write(encodeAudioFrame(track: .meeting, hostTimeNs: ns, sampleRate: rate, channels: ch, samples: samples))
            guardRef.feed(samples: samples, sampleRate: rate, sourcePlaying: playingBox.get())
        }
        let micCapture = MicCapture(micDeviceUID: micDeviceUID) { samples, ns, rate, ch in
            writer.write(encodeAudioFrame(track: .mic, hostTimeNs: ns, sampleRate: rate, channels: ch, samples: samples))
        }

        do {
            try tap.start()
            try micCapture.start()
        } catch {
            tap.stop()
            micCapture.stop()
            emit(Event.error(code: "fatal_capture", message: "failed to start capture"))
            return
        }

        meetingTap = tap
        mic = micCapture
        activeApp = app
        activeProcessIDs = procs.map { $0.objectID }
        playing.set(procs.contains { $0.isRunningOutput })

        emit(Event.captureStarted(sourceId: app.id, sampleRate: tap.sampleRate, channels: tap.channels))
    }

    private func stopCapture(emitStopped: Bool) {
        let hadActive = activeApp
        meetingTap?.stop()
        mic?.stop()
        meetingTap = nil
        mic = nil
        activeApp = nil
        activeProcessIDs = []
        playing.set(false)
        if emitStopped, let hadActive {
            emit(Event.captureStopped(sourceId: hadActive.id))
        }
    }

    private func teardown() {
        stopCapture(emitStopped: false)
        watcher.stop()
        Cleanup.reapOrphanAggregates()
    }

    // MARK: - source monitor

    private func startMonitor() {
        let timer = DispatchSource.makeTimerSource(queue: control)
        timer.schedule(deadline: .now() + 1.0, repeating: 1.0)
        timer.setEventHandler { [weak self] in self?.monitorTick() }
        timer.resume()
        monitorTimer = timer
    }

    private func monitorTick() {
        let all = ProcessList.current()

        // Emit source_started / source_stopped on producing-audio transitions.
        for app in config.apps {
            let procs = ProcessList.processes(for: app, in: all)
            let nowProducing = procs.contains { $0.isRunningOutput }
            let wasProducing = producingAudio[app.id] ?? false
            if nowProducing != wasProducing {
                let bundleID = procs.first?.bundleID ?? app.bundleIds.first ?? ""
                if nowProducing {
                    emit(Event.sourceStarted(sourceId: app.id, bundleId: bundleID, name: app.name))
                } else {
                    emit(Event.sourceStopped(sourceId: app.id, bundleId: bundleID, name: app.name))
                }
                producingAudio[app.id] = nowProducing
            }
        }

        // Keep the active capture's process set and playing flag current (handles the
        // meeting app restarting mid-call).
        if let app = activeApp {
            let procs = ProcessList.processes(for: app, in: all)
            playing.set(procs.contains { $0.isRunningOutput })
            let ids = procs.map { $0.objectID }
            if !ids.isEmpty, Set(ids) != Set(activeProcessIDs) {
                do {
                    try meetingTap?.updateProcesses(ids)
                    activeProcessIDs = ids
                } catch {
                    emit(Event.error(code: "internal", message: "failed to rebind tap"))
                }
            }
        }
    }

    // MARK: - device changes

    private func handleOutputChange() {
        emit(Event.deviceChanged(kind: "output"))
        guard meetingTap != nil else { return }
        do {
            try meetingTap?.restartForOutputChange()
        } catch {
            emit(Event.error(code: "internal", message: "failed to re-point capture after device change"))
        }
    }

    private func handleWake() {
        guard meetingTap != nil else { return }
        do {
            try meetingTap?.restartForOutputChange()
        } catch {
            emit(Event.error(code: "internal", message: "failed to resume capture after wake"))
        }
    }

    // MARK: - output

    private func emit(_ json: Data, track: Track = .meeting) {
        out.write(encodeEventFrame(json: json, hostTimeNs: Clock.nowNs(), track: track))
    }
}
