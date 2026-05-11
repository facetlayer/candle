import Foundation
import Network
import AppKit

/// Loopback HTTP introspection server. Binds 127.0.0.1 only.
/// Enabled when CANDLE_DEBUG_SERVER=1 (default in DEBUG builds).
/// Default port 4044 (override with CANDLE_DEBUG_PORT).
@MainActor
final class DebugServer {
    static let shared = DebugServer()

    private var listener: NWListener?
    private weak var store: AppStore?
    private(set) var port: UInt16 = 0

    func start(store: AppStore) {
        self.store = store

        let env = ProcessInfo.processInfo.environment
        #if DEBUG
        let defaultEnabled = true
        #else
        let defaultEnabled = false
        #endif
        let enabled = (env["CANDLE_DEBUG_SERVER"].map { $0 == "1" }) ?? defaultEnabled
        guard enabled else { return }

        let requestedPort = UInt16(env["CANDLE_DEBUG_PORT"] ?? "4044") ?? 4044

        do {
            let params = NWParameters.tcp
            params.requiredLocalEndpoint = NWEndpoint.hostPort(
                host: .ipv4(.loopback),
                port: NWEndpoint.Port(rawValue: requestedPort) ?? .any
            )
            let listener = try NWListener(using: params)
            self.listener = listener
            listener.newConnectionHandler = { [weak self] conn in
                Task { @MainActor in self?.handle(conn) }
            }
            listener.stateUpdateHandler = { [weak self] state in
                Task { @MainActor in
                    guard let self else { return }
                    if case .ready = state, let p = self.listener?.port?.rawValue {
                        self.port = p
                        NSLog("[swift-gui] Debug server listening on http://127.0.0.1:\(p)")
                    }
                    if case .failed(let err) = state {
                        NSLog("[swift-gui] Debug server failed: \(err)")
                    }
                }
            }
            listener.start(queue: .main)
        } catch {
            NSLog("[swift-gui] Debug server start error: \(error)")
        }
    }

    private func handle(_ conn: NWConnection) {
        conn.start(queue: .main)
        receive(conn, accumulated: Data())
    }

    private func receive(_ conn: NWConnection, accumulated: Data) {
        conn.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { [weak self] data, _, isComplete, error in
            var buf = accumulated
            if let data, !data.isEmpty { buf.append(data) }
            if let err = error {
                NSLog("[swift-gui] recv err: \(err)"); conn.cancel(); return
            }
            if let req = HTTPRequest.parse(buf) {
                Task { @MainActor in
                    guard let self else { conn.cancel(); return }
                    let resp = await self.route(req)
                    conn.send(content: resp.bytes(), completion: .contentProcessed { _ in
                        conn.cancel()
                    })
                }
            } else if isComplete {
                conn.cancel()
            } else {
                Task { @MainActor in self?.receive(conn, accumulated: buf) }
            }
        }
    }

    private func route(_ req: HTTPRequest) async -> HTTPResponse {
        switch (req.method, req.path) {
        case ("GET", "/"):
            return .json(["endpoints": [
                "GET /state",
                "GET /screen",
                "POST /action  {\"type\": \"...\", ...}"
            ]])
        case ("GET", "/state"):
            return .json(snapshot())
        case ("GET", "/screen"):
            if let png = captureWindowPNG() {
                return HTTPResponse(status: 200, headers: ["Content-Type": "image/png"], body: png)
            }
            return .json(["error": "capture failed"], status: 500)
        case ("POST", "/action"):
            return await handleAction(body: req.body)
        default:
            return .json(["error": "not found", "path": req.path], status: 404)
        }
    }

    // MARK: - State snapshot

    private func snapshot() -> [String: Any] {
        guard let s = store else { return ["error": "no store"] }
        return [
            "loading": s.loading,
            "errorMessage": s.errorMessage as Any,
            "autoScroll": s.autoScroll,
            "actionInProgress": s.actionInProgress as Any,
            "selected": s.selected.map { ["serviceName": $0.serviceName, "projectDir": $0.projectDir] } as Any,
            "services": s.services.map { serviceDict($0) },
            "groups": s.groups.map { g in
                [
                    "projectDir": g.projectDir,
                    "displayName": g.displayName,
                    "services": g.services.map { serviceDict($0) }
                ] as [String: Any]
            },
            "logCount": s.logs.count,
            "lastLogId": s.logs.last?.id as Any,
            "logs": s.logs.suffix(50).map { logDict($0) }
        ]
    }

    private func serviceDict(_ s: ServiceProcess) -> [String: Any] {
        [
            "serviceName": s.serviceName,
            "projectDir": s.projectDir,
            "pid": s.pid,
            "uptime": s.uptime,
            "status": s.status,
            "isRunning": s.isRunning
        ]
    }

    private func logDict(_ e: LogEntry) -> [String: Any] {
        [
            "id": e.id,
            "timestamp": e.timestamp,
            "log_type": e.log_type,
            "command_name": e.command_name,
            "content": (e.content ?? "") as Any
        ]
    }

    // MARK: - Actions

    private func handleAction(body: Data) async -> HTTPResponse {
        guard let store else { return .json(["error": "no store"], status: 500) }
        guard let obj = try? JSONSerialization.jsonObject(with: body) as? [String: Any],
              let type = obj["type"] as? String else {
            return .json(["error": "bad body"], status: 400)
        }
        switch type {
        case "selectService":
            if obj["name"] is NSNull {
                store.select(nil)
            } else {
                guard let name = obj["name"] as? String,
                      let dir = obj["projectDir"] as? String else {
                    return .json(["error": "missing name/projectDir"], status: 400)
                }
                store.select(SelectedService(serviceName: name, projectDir: dir))
                await store.refreshLogs()
            }
        case "deselect":
            store.select(nil)
        case "refresh":
            await store.refreshServices()
        case "refreshLogs":
            await store.refreshLogs()
        case "setAutoScroll":
            guard let v = obj["value"] as? Bool else {
                return .json(["error": "missing value"], status: 400)
            }
            store.autoScroll = v
        case "start", "restart", "kill":
            guard let name = obj["name"] as? String,
                  let dir = obj["projectDir"] as? String else {
                return .json(["error": "missing name/projectDir"], status: 400)
            }
            guard let svc = store.services.first(where: { $0.serviceName == name && $0.projectDir == dir }) else {
                return .json(["error": "service not found"], status: 404)
            }
            let action: ServiceAction = (type == "start") ? .start : (type == "restart" ? .restart : .kill)
            await store.performAction(action, on: svc)
        case "openInBrowser":
            guard let name = obj["name"] as? String,
                  let dir = obj["projectDir"] as? String else {
                return .json(["error": "missing name/projectDir"], status: 400)
            }
            guard let svc = store.services.first(where: { $0.serviceName == name && $0.projectDir == dir }) else {
                return .json(["error": "service not found"], status: 404)
            }
            await store.openInBrowser(svc)
        case "dismissError":
            store.errorMessage = nil
        default:
            return .json(["error": "unknown action", "type": type], status: 400)
        }
        return .json(["ok": true, "state": snapshot()])
    }

    // MARK: - Window capture

    private func captureWindowPNG() -> Data? {
        guard let win = NSApp.windows.first(where: { $0.isVisible }) else { return nil }
        let windowID = CGWindowID(win.windowNumber)
        guard let cg = CGWindowListCreateImage(.null,
                                               .optionIncludingWindow,
                                               windowID,
                                               [.boundsIgnoreFraming, .nominalResolution]) else { return nil }
        let rep = NSBitmapImageRep(cgImage: cg)
        return rep.representation(using: .png, properties: [:])
    }
}

// MARK: - Tiny HTTP

struct HTTPRequest {
    let method: String
    let path: String
    let query: [String: String]
    let headers: [String: String]
    let body: Data

    static func parse(_ data: Data) -> HTTPRequest? {
        guard let headerEnd = data.range(of: Data("\r\n\r\n".utf8)) else { return nil }
        let headerData = data.subdata(in: 0..<headerEnd.lowerBound)
        guard let headerStr = String(data: headerData, encoding: .utf8) else { return nil }
        let lines = headerStr.split(separator: "\r\n", omittingEmptySubsequences: false).map(String.init)
        guard let firstLine = lines.first else { return nil }
        let parts = firstLine.split(separator: " ").map(String.init)
        guard parts.count >= 2 else { return nil }
        let method = parts[0]
        let target = parts[1]
        var path = target; var query: [String: String] = [:]
        if let qIdx = target.firstIndex(of: "?") {
            path = String(target[..<qIdx])
            let qs = String(target[target.index(after: qIdx)...])
            for kv in qs.split(separator: "&") {
                let pair = kv.split(separator: "=", maxSplits: 1).map(String.init)
                if pair.count == 2 { query[pair[0]] = pair[1] }
            }
        }
        var headers: [String: String] = [:]
        for line in lines.dropFirst() {
            if let colon = line.firstIndex(of: ":") {
                let k = String(line[..<colon]).lowercased()
                let v = String(line[line.index(after: colon)...]).trimmingCharacters(in: .whitespaces)
                headers[k] = v
            }
        }
        let bodyStart = headerEnd.upperBound
        let contentLength = Int(headers["content-length"] ?? "0") ?? 0
        if data.count - bodyStart < contentLength { return nil }
        let body = data.subdata(in: bodyStart..<(bodyStart + contentLength))
        return HTTPRequest(method: method, path: path, query: query, headers: headers, body: body)
    }
}

struct HTTPResponse {
    let status: Int
    let headers: [String: String]
    let body: Data

    static func json(_ value: Any, status: Int = 200) -> HTTPResponse {
        let data = (try? JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys, .fragmentsAllowed])) ?? Data("{}".utf8)
        return HTTPResponse(status: status,
                            headers: ["Content-Type": "application/json"],
                            body: data)
    }

    func bytes() -> Data {
        let reason = HTTPResponse.reasonFor(status)
        var head = "HTTP/1.1 \(status) \(reason)\r\n"
        var hs = headers
        hs["Content-Length"] = String(body.count)
        hs["Connection"] = "close"
        for (k, v) in hs { head += "\(k): \(v)\r\n" }
        head += "\r\n"
        var out = Data(head.utf8)
        out.append(body)
        return out
    }

    private static func reasonFor(_ status: Int) -> String {
        switch status {
        case 200: return "OK"
        case 400: return "Bad Request"
        case 404: return "Not Found"
        case 500: return "Internal Server Error"
        default: return "OK"
        }
    }
}
