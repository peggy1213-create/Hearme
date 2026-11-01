import Foundation

// Entry point. With no arguments the helper runs in protocol mode (JSON commands on
// stdin, binary frames on stdout) as the desktop core's sidecar. `record` is a debug
// subcommand; `--help` prints usage. See packages/shared/CAPTURE_HELPER.md.

let arguments = Array(CommandLine.arguments.dropFirst())

func parseRecordOptions(_ args: [String]) -> Recorder.Options? {
    var bundle: String?
    var seconds = 30.0
    var outPath = "./out"
    var index = 0
    while index < args.count {
        switch args[index] {
        case "--bundle":
            index += 1
            if index < args.count { bundle = args[index] }
        case "--seconds":
            index += 1
            if index < args.count, let value = Double(args[index]) { seconds = value }
        case "--out":
            index += 1
            if index < args.count { outPath = args[index] }
        default:
            break
        }
        index += 1
    }
    guard let bundle else { return nil }
    return Recorder.Options(
        bundlePrefix: bundle,
        seconds: seconds,
        outDir: URL(fileURLWithPath: outPath)
    )
}

switch arguments.first {
case "record":
    guard let options = parseRecordOptions(Array(arguments.dropFirst())) else {
        logStderr("usage: tapcapture record --bundle <bundle-id> --seconds <n> --out <dir>")
        exit(2)
    }
    exit(Recorder.run(options))

case "--help", "-h", "help":
    print(usageText())
    exit(0)

case .none:
    // Protocol mode. Runs the main run loop forever until stop/EOF/signal.
    Session().run()

case .some(let unknown):
    logStderr("unknown argument: \(unknown)")
    print(usageText())
    exit(2)
}
