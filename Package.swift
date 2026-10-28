// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "FnPilot",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "FnPilotCore", targets: ["FnPilotCore"]),
        .executable(name: "FnPilot", targets: ["FnPilotApp"]),
        .executable(name: "fnpilot-bench", targets: ["FnPilotBench"]),
        .executable(name: "fnpilot-speech-bench", targets: ["FnPilotSpeechBench"]),
        .executable(name: "fnpilot-cli", targets: ["FnPilotCLI"])
    ],
    dependencies: [.package(url: "https://github.com/argmaxinc/WhisperKit.git", exact: "0.9.4")],
    targets: [
        .target(name: "FnPilotCore"),
        .executableTarget(name: "FnPilotApp", dependencies: ["FnPilotCore", .product(name: "WhisperKit", package: "WhisperKit")]),
        .executableTarget(name: "FnPilotBench", dependencies: ["FnPilotCore"]),
        .executableTarget(name: "FnPilotSpeechBench", dependencies: ["FnPilotCore", .product(name: "WhisperKit", package: "WhisperKit")]),
        .executableTarget(name: "FnPilotCLI", dependencies: ["FnPilotCore"]),
        .testTarget(name: "FnPilotCoreTests", dependencies: ["FnPilotCore"]),
        .testTarget(name: "FnPilotAppTests", dependencies: ["FnPilotApp"])
    ]
)
