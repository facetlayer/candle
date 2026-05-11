import SwiftUI

@MainActor
final class AppStore: ObservableObject {
    @Published var services: [ServiceProcess] = []
    @Published var loading = true
    @Published var errorMessage: String? = nil
    @Published var selected: SelectedService? = nil

    @Published var logs: [LogEntry] = []
    @Published var autoScroll: Bool = true

    @Published var actionInProgress: String? = nil

    private var lastLogId: Int = 0
    private var currentLogKey: String = ""
    private var serviceTask: Task<Void, Never>? = nil
    private var logTask: Task<Void, Never>? = nil

    var groups: [ProjectGroup] { groupByProject(services) }

    func startPolling() {
        serviceTask?.cancel()
        serviceTask = Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                await self.refreshServices()
                try? await Task.sleep(nanoseconds: 2_000_000_000)
            }
        }

        logTask?.cancel()
        logTask = Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                await self.refreshLogs()
                try? await Task.sleep(nanoseconds: 1_000_000_000)
            }
        }
    }

    func stopPolling() {
        serviceTask?.cancel()
        logTask?.cancel()
    }

    func select(_ svc: SelectedService?) {
        selected = svc
        let key = "\(svc?.projectDir ?? ""):\(svc?.serviceName ?? "")"
        if key != currentLogKey {
            currentLogKey = key
            logs = []
            lastLogId = 0
        }
    }

    func refreshServices() async {
        do {
            let items = try await CandleApi.shared.listServices()
            services = items
            errorMessage = nil
        } catch {
            errorMessage = "Failed to fetch services: \(error.localizedDescription)"
        }
        loading = false
    }

    func refreshLogs() async {
        guard let sel = selected else { return }
        do {
            let after = lastLogId > 0 ? lastLogId : nil
            let limit = lastLogId == 0 ? 200 : nil
            let new = try await CandleApi.shared.getLogs(
                name: sel.serviceName, projectDir: sel.projectDir,
                afterLogId: after, limit: limit
            )
            guard !new.isEmpty else { return }
            lastLogId = new.last?.id ?? lastLogId
            var combined = logs + new
            if combined.count > 2000 { combined = Array(combined.suffix(2000)) }
            logs = combined
        } catch {
            // Silent during polling, like the React version.
        }
    }

    func performAction(_ action: ServiceAction, on svc: ServiceProcess) async {
        let key = "\(svc.projectDir):\(svc.serviceName):\(action.rawValue)"
        actionInProgress = key
        defer { actionInProgress = nil }
        do {
            switch action {
            case .start:   try await CandleApi.shared.startService(name: svc.serviceName, projectDir: svc.projectDir)
            case .restart: try await CandleApi.shared.restartService(name: svc.serviceName, projectDir: svc.projectDir)
            case .kill:    try await CandleApi.shared.killService(name: svc.serviceName, projectDir: svc.projectDir)
            }
            await refreshServices()
        } catch {
            errorMessage = "Failed to \(action.rawValue) '\(svc.serviceName)': \(error.localizedDescription)"
        }
    }

    func openInBrowser(_ svc: ServiceProcess) async {
        let key = "\(svc.projectDir):\(svc.serviceName):open"
        actionInProgress = key
        defer { actionInProgress = nil }
        do {
            let url = try await CandleApi.shared.getServiceUrl(name: svc.serviceName, projectDir: svc.projectDir)
            if let urlStr = url, let nsUrl = URL(string: urlStr) {
                NSWorkspace.shared.open(nsUrl)
            } else {
                errorMessage = "No open port found for '\(svc.serviceName)'"
            }
        } catch {
            errorMessage = "Failed to get URL for '\(svc.serviceName)': \(error.localizedDescription)"
        }
    }

    func isInProgress(_ svc: ServiceProcess, action: String) -> Bool {
        actionInProgress == "\(svc.projectDir):\(svc.serviceName):\(action)"
    }
}

enum ServiceAction: String {
    case start, restart, kill
}
