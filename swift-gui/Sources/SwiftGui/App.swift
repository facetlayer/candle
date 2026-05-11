import SwiftUI

@main
struct SwiftGuiApp: App {
    @StateObject private var store = AppStore()

    var body: some Scene {
        WindowGroup("Candle") {
            ContentView()
                .environmentObject(store)
                .frame(minWidth: 900, minHeight: 600)
                .preferredColorScheme(.dark)
                .task {
                    store.startPolling()
                    DebugServer.shared.start(store: store)
                }
                .onDisappear { store.stopPolling() }
        }
        .windowResizability(.contentSize)
    }
}

struct ContentView: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        VStack(spacing: 0) {
            header
            if let err = store.errorMessage {
                Text(err)
                    .font(.system(size: 12))
                    .foregroundColor(Theme.statusStopped)
                    .padding(.horizontal, 16).padding(.vertical, 6)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Theme.statusStopped.opacity(0.10))
                    .accessibilityIdentifier("error-banner")
            }
            HStack(spacing: 0) {
                SidebarView()
                LogView()
            }
        }
        .background(Theme.bgPrimary)
        .accessibilityIdentifier("root")
    }

    private var header: some View {
        HStack(spacing: 8) {
            Image(systemName: "flame.fill")
                .foregroundColor(Theme.accent)
            Text("Candle")
                .font(.system(size: 14, weight: .semibold))
                .foregroundColor(Theme.textPrimary)
            Text("Process Manager")
                .font(.system(size: 11))
                .foregroundColor(Theme.textMuted)
            Spacer()
        }
        .padding(.horizontal, 16).padding(.vertical, 10)
        .background(Theme.bgSecondary)
        .overlay(Rectangle().frame(height: 1).foregroundColor(Theme.border), alignment: .bottom)
    }
}
