import Foundation

struct ServiceProcess: Identifiable, Hashable, Codable {
    let serviceName: String
    let projectDir: String
    let pid: Int
    let uptime: String
    let status: String
    let shell: String?
    let root: String?

    var id: String { "\(projectDir):\(serviceName):\(pid)" }
    var isRunning: Bool { status == "RUNNING" }
}

struct ServiceListResponse: Codable {
    let processes: [ServiceProcess]
}

struct ProjectGroup: Identifiable {
    let projectDir: String
    let displayName: String
    let services: [ServiceProcess]
    var id: String { projectDir }
}

enum LogType: Int, Codable {
    case stdout = 1
    case stderr = 2
    case processStartInitiated = 3
    case processStartFailed = 4
    case processStarted = 5
    case processExited = 6

    var lifecycleLabel: String? {
        switch self {
        case .processStartInitiated: return "[Starting...]"
        case .processStartFailed:    return "[Start failed]"
        case .processStarted:        return "[Process started]"
        case .processExited:         return "[Process exited]"
        default: return nil
        }
    }

    var isLifecycle: Bool { rawValue >= LogType.processStartInitiated.rawValue }
}

struct LogEntry: Identifiable, Hashable, Codable {
    let id: Int
    let command_name: String
    let content: String?
    let log_type: Int
    let timestamp: Double

    var type: LogType { LogType(rawValue: log_type) ?? .stdout }

    var displayContent: String {
        if type.isLifecycle {
            return content ?? type.lifecycleLabel ?? ""
        }
        return content ?? ""
    }
}

struct LogsResponse: Codable {
    let logs: [LogEntry]
}

struct UrlResponse: Codable {
    let url: String?
}

struct SelectedService: Equatable, Hashable {
    let serviceName: String
    let projectDir: String
}

func displayName(forProjectDir dir: String) -> String {
    let parts = dir.split(separator: "/")
    return parts.suffix(2).joined(separator: "/")
}

func groupByProject(_ processes: [ServiceProcess]) -> [ProjectGroup] {
    var map: [String: [ServiceProcess]] = [:]
    for proc in processes {
        map[proc.projectDir, default: []].append(proc)
    }
    return map.keys.sorted().map { dir in
        ProjectGroup(projectDir: dir, displayName: displayName(forProjectDir: dir), services: map[dir]!)
    }
}
