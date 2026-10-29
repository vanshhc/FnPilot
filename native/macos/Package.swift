// swift-tools-version: 5.10
import PackageDescription
let package = Package(
    name: "FnPilotSpeech", platforms: [.macOS(.v14)],
    products: [.library(name: "FnPilotSpeech", type: .static, targets: ["FnPilotSpeech"]), .executable(name: "fnpilot-whisperkit-bench", targets: ["FnPilotSpeechBench"])],
    dependencies: [.package(path: ".build/offline-whisperkit")],
    targets: [
        .target(name: "FnPilotSpeech", dependencies: [.product(name: "WhisperKit", package: "offline-whisperkit")]),
        .executableTarget(name: "FnPilotSpeechBench", dependencies: ["FnPilotSpeech", .product(name:"WhisperKit",package:"offline-whisperkit")]),
        .testTarget(name: "FnPilotSpeechTests", dependencies: ["FnPilotSpeech"])
    ])
