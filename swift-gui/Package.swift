// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "SwiftGui",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "SwiftGui", targets: ["SwiftGui"]),
    ],
    targets: [
        .executableTarget(
            name: "SwiftGui",
            path: "Sources/SwiftGui"
        ),
    ]
)
