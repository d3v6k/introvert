// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "pdf_render_maintained",
    platforms: [
        .macOS(.v10_15)
    ],
    products: [
        .library(
            name: "pdf_render_maintained",
            targets: ["pdf_render_maintained"]
        ),
    ],
    dependencies: [
        // Add any dependencies here if needed
    ],
    targets: [
        .target(
            name: "pdf_render_maintained",
            dependencies: [],
            path: "Sources"
        ),
    ]
)
