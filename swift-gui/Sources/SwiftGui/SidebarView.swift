import SwiftUI

struct SidebarView: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        VStack(spacing: 0) {
            header
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if store.loading {
                        Text("Loading services...")
                            .font(.system(size: 13))
                            .foregroundColor(Theme.textDim)
                            .padding(16)
                            .accessibilityIdentifier("loading-services")
                    } else if store.groups.isEmpty {
                        Text("No running services")
                            .font(.system(size: 13))
                            .foregroundColor(Theme.textDim)
                            .padding(16)
                            .accessibilityIdentifier("no-services")
                    } else {
                        ForEach(store.groups) { group in
                            ProjectGroupView(group: group)
                        }
                    }
                }
                .padding(.vertical, 4)
            }
        }
        .frame(width: 340)
        .background(Theme.bgSecondary)
        .overlay(Rectangle().frame(width: 1).foregroundColor(Theme.border), alignment: .trailing)
    }

    private var header: some View {
        HStack {
            Text("Services")
                .font(.system(size: 13, weight: .semibold))
                .foregroundColor(Theme.textPrimary)
            Spacer()
            Button(action: { Task { await store.refreshServices() } }) {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(Theme.textSecondary)
                    .frame(width: 24, height: 24)
                    .background(Theme.bgInput)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("refresh-services")
            .help("Refresh")
        }
        .padding(.horizontal, 14).padding(.vertical, 12)
        .background(Theme.bgSecondary)
        .overlay(Rectangle().frame(height: 1).foregroundColor(Theme.border), alignment: .bottom)
    }
}

struct ProjectGroupView: View {
    let group: ProjectGroup

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundColor(Theme.textDim)
                Text(group.displayName)
                    .font(.system(size: 11, weight: .semibold))
                    .tracking(0.4)
                    .foregroundColor(Theme.textMuted)
                Spacer()
                Text("\(group.services.count)")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundColor(Theme.textDim)
                    .padding(.horizontal, 6).padding(.vertical, 2)
                    .background(Theme.bgInput)
                    .clipShape(Capsule())
            }
            .padding(.horizontal, 14).padding(.top, 12).padding(.bottom, 6)
            .help(group.projectDir)

            ForEach(group.services) { svc in
                ServiceRow(service: svc)
            }
        }
    }
}

struct ServiceRow: View {
    @EnvironmentObject var store: AppStore
    let service: ServiceProcess
    @State private var hovered = false

    var body: some View {
        let selected = store.selected?.serviceName == service.serviceName
            && store.selected?.projectDir == service.projectDir
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(service.serviceName)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(selected ? .white : Theme.textSecondary)
                Spacer()
                StatusBadge(running: service.isRunning)
            }
            if service.isRunning {
                HStack(spacing: 12) {
                    Text("PID \(service.pid)")
                    Text("Up \(service.uptime)")
                }
                .font(.system(size: 11))
                .foregroundColor(Theme.textDim)
            }
            if service.isRunning {
                HStack(spacing: 6) {
                    ActionButton(label: progressLabel("open", default: "Open"),
                                 disabled: store.isInProgress(service, action: "open"),
                                 kind: .neutral) {
                        Task { await store.openInBrowser(service) }
                    }
                    .accessibilityIdentifier("open-\(service.serviceName)")
                    ActionButton(label: progressLabel("restart", default: "Restart"),
                                 disabled: store.isInProgress(service, action: "restart"),
                                 kind: .neutral) {
                        Task { await store.performAction(.restart, on: service) }
                    }
                    .accessibilityIdentifier("restart-\(service.serviceName)")
                    ActionButton(label: progressLabel("kill", default: "Kill"),
                                 disabled: store.isInProgress(service, action: "kill"),
                                 kind: .danger) {
                        Task { await store.performAction(.kill, on: service) }
                    }
                    .accessibilityIdentifier("kill-\(service.serviceName)")
                }
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(selected ? Theme.bgActive : (hovered ? Theme.bgHover : Color.clear))
        .overlay(
            Rectangle().frame(width: 3).foregroundColor(Theme.accent).opacity(selected ? 1 : 0),
            alignment: .leading
        )
        .contentShape(Rectangle())
        .onTapGesture {
            store.select(SelectedService(serviceName: service.serviceName, projectDir: service.projectDir))
        }
        .onHover { hovered = $0 }
        .accessibilityIdentifier("service-row-\(service.serviceName)")
    }

    private func progressLabel(_ action: String, default def: String) -> String {
        if store.isInProgress(service, action: action) {
            switch action {
            case "open": return "Opening..."
            case "restart": return "Restarting..."
            case "kill": return "Killing..."
            default: return def
            }
        }
        return def
    }
}

struct StatusBadge: View {
    let running: Bool
    var body: some View {
        Text(running ? "Running" : "Stopped")
            .font(.system(size: 10, weight: .semibold))
            .tracking(0.3)
            .foregroundColor(running ? Theme.statusRunning : Theme.statusStopped)
            .padding(.horizontal, 6).padding(.vertical, 2)
            .background((running ? Theme.statusRunning : Theme.statusStopped).opacity(0.12))
            .clipShape(Capsule())
    }
}

enum ActionKind { case neutral, danger }

struct ActionButton: View {
    let label: String
    let disabled: Bool
    let kind: ActionKind
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(label)
                .font(.system(size: 11, weight: .medium))
                .foregroundColor(disabled ? Theme.textDim : (kind == .danger ? Theme.statusStopped : Theme.textPrimary))
                .padding(.horizontal, 8).padding(.vertical, 4)
                .background(Theme.bgInput)
                .overlay(RoundedRectangle(cornerRadius: 4).stroke(Theme.border, lineWidth: 1))
                .clipShape(RoundedRectangle(cornerRadius: 4))
        }
        .buttonStyle(.plain)
        .disabled(disabled)
    }
}
