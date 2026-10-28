import Foundation

public enum SpeechAudio {
    /// Input is mono, 16 kHz PCM. Validate the recording before adding any silence.
    public static func prepareForWhisper(_ samples: [Float]) throws -> [Float] {
        guard samples.count >= 1600 else { throw PilotError("Hold Fn while speaking, then release it.") }
        let rms = sqrt(samples.reduce(Float(0)) { $0 + $1 * $1 } / Float(samples.count))
        guard rms > 0.002 else { throw PilotError("No speech was detected.") }
        // WhisperKit 0.9.4 stops its decode loop one second before the clip end.
        // At or below one second it never enters that loop, even for audible speech.
        guard samples.count <= 16000 else { return samples }
        return samples + [Float](repeating: 0, count: 16001 - samples.count)
    }
}
