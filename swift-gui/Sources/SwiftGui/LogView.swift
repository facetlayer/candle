import SwiftUI

struct LogView: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        VStack(spacing: 0) {
            header
            if store.selected == nil {
                Spacer()
                Text("Select a service to view logs")
                    .font(.system(size: 13))
                    .foregroundColor(Theme.textDim)
                    .accessibilityIdentifier("logs-placeholder")
                Spacer()
            } else {
                logScroll
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.bgPrimary)
    }

    private var header: some View {
        HStack {
            Text("Logs")
                .font(.system(size: 13, weight: .semibold))
                .foregroundColor(Theme.textPrimary)
            if let sel = store.selected {
                Text(sel.serviceName)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(Theme.accent)
                    .padding(.horizontal, 8).padding(.vertical, 2)
                    .background(Theme.bgSecondary)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
            }
            Spacer()
            Toggle(isOn: $store.autoScroll) {
                Text("Auto-scroll")
                    .font(.system(size: 11))
                    .foregroundColor(Theme.textSecondary)
            }
            .toggleStyle(.switch)
            .controlSize(.small)
            .accessibilityIdentifier("auto-scroll-toggle")
        }
        .padding(.horizontal, 16).padding(.vertical, 10)
        .background(Theme.bgSecondary)
        .overlay(Rectangle().frame(height: 1).foregroundColor(Theme.border), alignment: .bottom)
    }

    private var logScroll: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if store.logs.isEmpty {
                        Text("No logs available")
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundColor(Theme.logLifecycle)
                            .padding(8)
                            .accessibilityIdentifier("no-logs")
                    }
                    ForEach(store.logs) { entry in
                        LogRow(entry: entry).id(entry.id)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.vertical, 6)
            }
            .background(Theme.bgPrimary)
            .accessibilityIdentifier("log-content")
            .onChange(of: store.logs.last?.id) { _ in
                if store.autoScroll, let last = store.logs.last {
                    withAnimation(.easeOut(duration: 0.1)) {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
        }
    }
}

struct LogRow: View {
    let entry: LogEntry

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Text(formatTimestamp(entry.timestamp))
                .font(.system(size: 11, design: .monospaced))
                .foregroundColor(Theme.textDim)
            Text(entry.displayContent)
                .font(.system(size: 12, design: .monospaced))
                .foregroundColor(color(for: entry.type))
                .frame(maxWidth: .infinity, alignment: .leading)
                .textSelection(.enabled)
        }
        .padding(.horizontal, 12).padding(.vertical, 1)
    }

    private func color(for t: LogType) -> Color {
        switch t {
        case .stdout: return Theme.logStdout
        case .stderr: return Theme.logStderr
        default: return Theme.logLifecycle
        }
    }

    private func formatTimestamp(_ unix: Double) -> String {
        let date = Date(timeIntervalSince1970: unix)
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss"
        return f.string(from: date)
    }
}
