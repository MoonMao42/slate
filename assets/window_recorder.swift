import Foundation
import ScreenCaptureKit
import AVFoundation
import CoreMedia
import AppKit

class WindowRecorder: NSObject, SCStreamOutput, SCStreamDelegate {
    var stream: SCStream?
    var assetWriter: AVAssetWriter?
    var videoInput: AVAssetWriterInput?
    var isRecording = false
    let outputURL: URL
    let windowID: CGWindowID
    let videoQueue = DispatchQueue(label: "com.recorder.videoQueue")
    var sessionStarted = false

    init(windowID: CGWindowID, outputURL: URL) {
        self.windowID = windowID
        self.outputURL = outputURL
    }

    func start() async throws {
        let availableContent = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let window = availableContent.windows.first(where: { $0.windowID == self.windowID }) else {
            print("Window \(self.windowID) not found"); exit(1)
        }
        let filter = SCContentFilter(desktopIndependentWindow: window)
        let config = SCStreamConfiguration()
        let scale = NSScreen.main?.backingScaleFactor ?? 2.0
        config.width = max(Int(window.frame.width * scale), 128)
        config.height = max(Int(window.frame.height * scale), 128)
        config.minimumFrameInterval = CMTime(value: 1, timescale: 60)
        config.queueDepth = 8
        config.showsCursor = true
        config.capturesAudio = false

        assetWriter = try AVAssetWriter(outputURL: outputURL, fileType: .mov)
        let compression: [String: Any] = [
            AVVideoAverageBitRateKey: max(config.width * config.height * 8, 8_000_000),
            AVVideoExpectedSourceFrameRateKey: 60,
            AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel
        ]
        let vs: [String: Any] = [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: config.width,
            AVVideoHeightKey: config.height,
            AVVideoCompressionPropertiesKey: compression
        ]
        videoInput = AVAssetWriterInput(mediaType: .video, outputSettings: vs)
        videoInput?.expectsMediaDataInRealTime = true
        assetWriter!.add(videoInput!)
        assetWriter!.startWriting()

        stream = SCStream(filter: filter, configuration: config, delegate: self)
        try stream?.addStreamOutput(self, type: .screen, sampleHandlerQueue: videoQueue)
        try await stream?.startCapture()
        isRecording = true
        print("Recording window \(windowID)...")
    }

    func stop() async {
        guard isRecording else { return }
        isRecording = false
        try? await stream?.stopCapture()
        videoInput?.markAsFinished()
        if let aw = assetWriter, aw.status == .writing {
            await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
                aw.finishWriting { c.resume() }
            }
        }
        print("Saved: \(outputURL.path)")
        exit(0)
    }

    func stream(_ s: SCStream, didOutputSampleBuffer sb: CMSampleBuffer, of type: SCStreamOutputType) {
        guard isRecording, type == .screen, let vi = videoInput, vi.isReadyForMoreMediaData else { return }
        guard let aa = CMSampleBufferGetSampleAttachmentsArray(sb, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let a = aa.first, let rv = a[.status] as? Int, let st = SCFrameStatus(rawValue: rv), st == .complete else { return }
        let pts = CMSampleBufferGetPresentationTimeStamp(sb)
        if pts.isValid {
            if !sessionStarted { assetWriter?.startSession(atSourceTime: pts); sessionStarted = true }
            vi.append(sb)
        }
    }
    func stream(_ s: SCStream, didStopWithError e: Error) { print("Error: \(e)"); exit(1) }
}

func listWindows() async {
    let content = try! await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
    for w in content.windows where w.frame.width > 50 && w.frame.height > 50 {
        let app = w.owningApplication?.applicationName ?? "?"
        let title = w.title ?? ""
        print("\(w.windowID)\t\(app)\t\(title.prefix(50))")
    }
}

// Initialize macOS graphics subsystem (required for ScreenCaptureKit from CLI)
let _ = NSApplication.shared

let args = CommandLine.arguments
if args.count >= 2 && args[1] == "list" {
    Task { await listWindows(); exit(0) }
    dispatchMain()
} else if args.count >= 3, let wid = UInt32(args[1]) {
    let url = URL(fileURLWithPath: args[2])
    try? FileManager.default.removeItem(at: url)
    let rec = WindowRecorder(windowID: wid, outputURL: url)
    let src = DispatchSource.makeSignalSource(signal: SIGINT, queue: .main)
    src.setEventHandler { Task { await rec.stop() } }
    src.resume()
    signal(SIGINT, SIG_IGN)
    Task { try await rec.start() }
    dispatchMain()
} else {
    print("Usage: window_recorder <windowID> <output.mov>")
    print("       window_recorder list")
    exit(0)
}
