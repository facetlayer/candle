import SwiftUI

extension Color {
    init(hex: UInt32) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255.0,
            green: Double((hex >> 8) & 0xFF) / 255.0,
            blue: Double(hex & 0xFF) / 255.0,
            opacity: 1.0
        )
    }
}

enum Theme {
    static let bgPrimary     = Color(hex: 0x1a1a2e)
    static let bgSecondary   = Color(hex: 0x16213e)
    static let bgHover       = Color(hex: 0x1f2b47)
    static let bgActive      = Color(hex: 0x2a3a5c)
    static let bgInput       = Color(hex: 0x0f1629)
    static let border        = Color(hex: 0x2a2a4a)
    static let borderFocus   = Color(hex: 0xa78bfa)
    static let accent        = Color(hex: 0xa78bfa)
    static let accentHover   = Color(hex: 0x8b6fdf)
    static let textPrimary   = Color(hex: 0xe0e0e8)
    static let textSecondary = Color(hex: 0xc0c0d0)
    static let textMuted     = Color(hex: 0x8888aa)
    static let textDim       = Color(hex: 0x6b7a8d)
    static let statusRunning = Color(hex: 0x4ade80)
    static let statusStopped = Color(hex: 0xf87171)
    static let logStdout     = Color(hex: 0xc0c0d0)
    static let logStderr     = Color(hex: 0xfca5a5)
    static let logLifecycle  = Color(hex: 0xa78bfa)
}
