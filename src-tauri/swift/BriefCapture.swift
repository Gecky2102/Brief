import AVFoundation
import Foundation
import ScreenCaptureKit

/// Whisper vuole PCM mono a 16 kHz: convertiamo subito, così su disco finisce
/// già il formato buono per la trascrizione e non serve un secondo passaggio.
private let targetSampleRate = 16000.0
private let levelIntervalMs: Int64 = 50

private let trackMic: Int32 = 0
private let trackSystem: Int32 = 1

private enum CaptureError: Int32 {
    case ok = 0
    case microphoneDenied = 1
    case screenDenied = 2
    case engineFailed = 3
    case alreadyRunning = 4
    case notRunning = 5
    case writeFailed = 6
    case unsupportedOS = 7
}

/// WAV PCM 16 bit mono. L'header viene riscritto alla chiusura, quando le
/// dimensioni finali sono note.
private final class WavWriter {
    private let handle: FileHandle
    private let url: URL
    private var dataBytes: UInt32 = 0

    init(url: URL) throws {
        self.url = url
        FileManager.default.createFile(atPath: url.path, contents: nil)
        handle = try FileHandle(forWritingTo: url)
        try handle.write(contentsOf: WavWriter.header(dataBytes: 0))
    }

    private static func header(dataBytes: UInt32) -> Data {
        let sampleRate = UInt32(targetSampleRate)
        let byteRate = sampleRate * 2
        var data = Data()

        func append<T: FixedWidthInteger>(_ value: T) {
            withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) }
        }

        data.append(contentsOf: Array("RIFF".utf8))
        append(UInt32(36 &+ dataBytes))
        data.append(contentsOf: Array("WAVEfmt ".utf8))
        append(UInt32(16))
        append(UInt16(1))  // PCM
        append(UInt16(1))  // mono
        append(sampleRate)
        append(byteRate)
        append(UInt16(2))  // block align
        append(UInt16(16))  // bit depth
        data.append(contentsOf: Array("data".utf8))
        append(dataBytes)
        return data
    }

    func write(_ samples: UnsafeBufferPointer<Int16>) throws {
        guard let base = samples.baseAddress, !samples.isEmpty else { return }
        let data = Data(bytes: base, count: samples.count * 2)
        try handle.write(contentsOf: data)
        dataBytes &+= UInt32(data.count)
    }

    func close() {
        try? handle.seek(toOffset: 0)
        try? handle.write(contentsOf: WavWriter.header(dataBytes: dataBytes))
        try? handle.close()
    }

    var byteCount: UInt32 { dataBytes }
}

/// Converte i buffer in arrivo (formato hardware, di solito 44,1 o 48 kHz
/// stereo) verso mono 16 kHz, scrive su WAV e riporta il livello RMS.
private final class TrackSink {
    private let track: Int32
    private let writer: WavWriter
    private let queue: DispatchQueue
    private var converter: AVAudioConverter?
    private var sourceFormat: AVAudioFormat?
    private let outputFormat: AVAudioFormat
    private var lastLevelMs: Int64 = 0
    private var peak: Float = 0

    init(track: Int32, url: URL) throws {
        self.track = track
        writer = try WavWriter(url: url)
        queue = DispatchQueue(label: "it.gmasiero.brief.sink.\(track)")
        outputFormat = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: targetSampleRate,
            channels: 1,
            interleaved: true)!
    }

    func append(
        _ buffer: AVAudioPCMBuffer, at elapsedMs: Int64, notify: LevelCallback?,
        samples emitSamples: SamplesCallback?
    ) {
        queue.sync {
            guard let converted = convert(buffer) else { return }
            guard let channel = converted.int16ChannelData, converted.frameLength > 0 else {
                return
            }

            let count = Int(converted.frameLength)
            let samples = UnsafeBufferPointer(start: channel[0], count: count)
            try? writer.write(samples)
            emitSamples?(track, channel[0], Int32(count), elapsedMs)

            var sumSquares: Double = 0
            for sample in samples {
                let normalized = Double(sample) / 32768.0
                sumSquares += normalized * normalized
            }
            let rms = count > 0 ? Float((sumSquares / Double(count)).squareRoot()) : 0
            peak = max(peak, rms)

            if elapsedMs - lastLevelMs >= levelIntervalMs {
                lastLevelMs = elapsedMs
                notify?(track, peak, elapsedMs)
                peak = 0
            }
        }
    }

    private func convert(_ buffer: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
        if converter == nil || sourceFormat != buffer.format {
            sourceFormat = buffer.format
            converter = AVAudioConverter(from: buffer.format, to: outputFormat)
        }
        guard let converter else { return nil }

        let ratio = outputFormat.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 1024
        guard
            let output = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: capacity)
        else { return nil }

        var consumed = false
        var error: NSError?
        converter.convert(to: output, error: &error) { _, status in
            if consumed {
                status.pointee = .noDataNow
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return buffer
        }

        if error != nil || output.frameLength == 0 { return nil }
        return output
    }

    func close() -> UInt32 {
        queue.sync {
            writer.close()
            return writer.byteCount
        }
    }
}

public typealias LevelCallback = @convention(c) (Int32, Float, Int64) -> Void

/// (traccia, campioni, numero di campioni, millisecondi di inizio del blocco).
/// I campioni sono validi solo per la durata della chiamata: chi riceve deve
/// copiarli.
public typealias SamplesCallback = @convention(c) (Int32, UnsafePointer<Int16>, Int32, Int64)
    -> Void

@available(macOS 13.0, *)
private final class Capture: NSObject, SCStreamOutput, SCStreamDelegate {
    private let engine = AVAudioEngine()
    private var stream: SCStream?
    private var micSink: TrackSink?
    private var systemSink: TrackSink?
    private var startedAt: DispatchTime?
    private var callback: LevelCallback?
    private var samplesCallback: SamplesCallback?

    private var elapsedMs: Int64 {
        guard let startedAt else { return 0 }
        let delta = DispatchTime.now().uptimeNanoseconds &- startedAt.uptimeNanoseconds
        return Int64(delta / 1_000_000)
    }

    func start(
        directory: URL, callback: @escaping LevelCallback,
        samples samplesCallback: @escaping SamplesCallback
    ) -> CaptureError {
        self.callback = callback
        self.samplesCallback = samplesCallback

        guard requestMicrophoneAccess() else { return .microphoneDenied }

        do {
            micSink = try TrackSink(track: trackMic, url: directory.appendingPathComponent("mic.wav"))
            systemSink = try TrackSink(
                track: trackSystem, url: directory.appendingPathComponent("system.wav"))
        } catch {
            return .writeFailed
        }

        startedAt = DispatchTime.now()

        if let failure = startMicrophone() { return failure }
        if let failure = startSystemAudio() {
            engine.stop()
            engine.inputNode.removeTap(onBus: 0)
            return failure
        }

        return .ok
    }

    private func requestMicrophoneAccess() -> Bool {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            return true
        case .notDetermined:
            let gate = DispatchSemaphore(value: 0)
            var granted = false
            AVCaptureDevice.requestAccess(for: .audio) { allowed in
                granted = allowed
                gate.signal()
            }
            return gate.wait(timeout: .now() + 60) == .success && granted
        default:
            return false
        }
    }

    private func startMicrophone() -> CaptureError? {
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0 else { return .engineFailed }

        input.installTap(onBus: 0, bufferSize: 4096, format: format) { [weak self] buffer, _ in
            guard let self else { return }
            self.micSink?.append(
                buffer, at: self.elapsedMs, notify: self.callback, samples: self.samplesCallback)
        }

        do {
            engine.prepare()
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            return .engineFailed
        }
        return nil
    }

    private func startSystemAudio() -> CaptureError? {
        let gate = DispatchSemaphore(value: 0)
        var result: CaptureError?

        Task {
            do {
                let content = try await SCShareableContent.excludingDesktopWindows(
                    false, onScreenWindowsOnly: false)
                guard let display = content.displays.first else {
                    result = .screenDenied
                    gate.signal()
                    return
                }

                let configuration = SCStreamConfiguration()
                configuration.capturesAudio = true
                configuration.sampleRate = 48000
                configuration.channelCount = 2
                // Senza questa esclusione l'app registrerebbe anche il proprio
                // output, creando un anello di feedback nella trascrizione.
                configuration.excludesCurrentProcessAudio = true
                // Lo stream richiede comunque una parte video: la teniamo al
                // minimo assoluto perché ci serve solo l'audio.
                configuration.width = 2
                configuration.height = 2
                configuration.minimumFrameInterval = CMTime(value: 1, timescale: 1)
                configuration.queueDepth = 3

                let filter = SCContentFilter(display: display, excludingWindows: [])
                let stream = SCStream(filter: filter, configuration: configuration, delegate: self)
                try stream.addStreamOutput(
                    self, type: .audio,
                    sampleHandlerQueue: DispatchQueue(label: "it.gmasiero.brief.system"))
                try await stream.startCapture()
                self.stream = stream
            } catch {
                result = .screenDenied
            }
            gate.signal()
        }

        _ = gate.wait(timeout: .now() + 70)
        return result
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio, CMSampleBufferDataIsReady(buffer) else { return }
        guard let description = CMSampleBufferGetFormatDescription(buffer),
            let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(description)
        else { return }

        var streamDescription = asbd.pointee
        guard let format = AVAudioFormat(streamDescription: &streamDescription) else { return }

        let frames = AVAudioFrameCount(CMSampleBufferGetNumSamples(buffer))
        guard frames > 0,
            let pcm = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames)
        else { return }
        pcm.frameLength = frames

        let status = CMSampleBufferCopyPCMDataIntoAudioBufferList(
            buffer, at: 0, frameCount: Int32(frames), into: pcm.mutableAudioBufferList)
        guard status == noErr else { return }

        systemSink?.append(pcm, at: elapsedMs, notify: callback, samples: samplesCallback)
    }

    func stop() -> Int64 {
        let duration = elapsedMs

        engine.inputNode.removeTap(onBus: 0)
        engine.stop()

        if let stream {
            let gate = DispatchSemaphore(value: 0)
            Task {
                try? await stream.stopCapture()
                gate.signal()
            }
            _ = gate.wait(timeout: .now() + 5)
        }
        stream = nil

        _ = micSink?.close()
        _ = systemSink?.close()
        micSink = nil
        systemSink = nil
        startedAt = nil
        callback = nil
        samplesCallback = nil

        return duration
    }
}

private let stateLock = NSLock()
private var active: AnyObject?

@_cdecl("brief_capture_start")
public func brief_capture_start(
    _ directory: UnsafePointer<CChar>, _ callback: @escaping LevelCallback,
    _ samplesCallback: @escaping SamplesCallback
) -> Int32 {
    guard #available(macOS 13.0, *) else { return CaptureError.unsupportedOS.rawValue }

    stateLock.lock()
    defer { stateLock.unlock() }
    guard active == nil else { return CaptureError.alreadyRunning.rawValue }

    let url = URL(fileURLWithPath: String(cString: directory), isDirectory: true)
    let capture = Capture()
    let result = capture.start(directory: url, callback: callback, samples: samplesCallback)
    if result == .ok { active = capture }
    return result.rawValue
}

@_cdecl("brief_capture_stop")
public func brief_capture_stop() -> Int64 {
    guard #available(macOS 13.0, *) else { return -1 }

    stateLock.lock()
    defer { stateLock.unlock() }
    guard let capture = active as? Capture else { return -1 }

    active = nil
    return capture.stop()
}

@_cdecl("brief_capture_is_running")
public func brief_capture_is_running() -> Int32 {
    stateLock.lock()
    defer { stateLock.unlock() }
    return active == nil ? 0 : 1
}
