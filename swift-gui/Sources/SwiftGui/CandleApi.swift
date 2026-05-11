import Foundation

enum CandleApiError: LocalizedError {
    case http(Int, String)
    case decoding(String)
    case transport(String)

    var errorDescription: String? {
        switch self {
        case .http(let code, let msg): return "HTTP \(code): \(msg)"
        case .decoding(let msg): return "Decode error: \(msg)"
        case .transport(let msg): return "Network error: \(msg)"
        }
    }
}

/// HTTP client for the candle GUI API (gui/src/services/candle-service.ts).
/// Endpoints are mounted under /api by the Prism framework.
/// Default port matches gui/.env. Override with CANDLE_API_URL env var.
actor CandleApi {
    static let shared = CandleApi()

    static func defaultBaseURL() -> URL {
        if let s = ProcessInfo.processInfo.environment["CANDLE_API_URL"], let u = URL(string: s) {
            return u
        }
        return URL(string: "http://localhost:4022/api")!
    }

    var baseURL: URL

    private let session: URLSession
    private let decoder: JSONDecoder

    init(baseURL: URL? = nil) {
        self.baseURL = baseURL ?? Self.defaultBaseURL()
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 5
        config.timeoutIntervalForResource = 10
        self.session = URLSession(configuration: config)
        self.decoder = JSONDecoder()
    }

    func setBaseURL(_ url: URL) {
        self.baseURL = url
    }

    func listServices() async throws -> [ServiceProcess] {
        let resp: ServiceListResponse = try await get("/services")
        return resp.processes
    }

    func getLogs(name: String, projectDir: String, afterLogId: Int?, limit: Int?) async throws -> [LogEntry] {
        var items: [URLQueryItem] = [URLQueryItem(name: "projectDir", value: projectDir)]
        if let after = afterLogId { items.append(URLQueryItem(name: "afterLogId", value: String(after))) }
        if let limit = limit { items.append(URLQueryItem(name: "limit", value: String(limit))) }
        let resp: LogsResponse = try await get("/services/\(escape(name))/logs", query: items)
        return resp.logs
    }

    func startService(name: String, projectDir: String) async throws {
        try await postVoid("/services/\(escape(name))/start", body: ["projectDir": projectDir])
    }

    func restartService(name: String, projectDir: String) async throws {
        try await postVoid("/services/\(escape(name))/restart", body: ["projectDir": projectDir])
    }

    func killService(name: String, projectDir: String) async throws {
        try await postVoid("/services/\(escape(name))/kill", body: ["projectDir": projectDir])
    }

    func getServiceUrl(name: String, projectDir: String) async throws -> String? {
        let items = [URLQueryItem(name: "projectDir", value: projectDir)]
        let resp: UrlResponse = try await get("/services/\(escape(name))/url", query: items)
        return resp.url
    }

    // MARK: - Internals

    private func escape(_ s: String) -> String {
        s.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? s
    }

    private func get<T: Decodable>(_ path: String, query: [URLQueryItem] = []) async throws -> T {
        var comps = URLComponents(url: baseURL.appendingPathComponent(path), resolvingAgainstBaseURL: false)!
        if !query.isEmpty { comps.queryItems = query }
        var req = URLRequest(url: comps.url!)
        req.httpMethod = "GET"
        return try await send(req)
    }

    private func postVoid(_ path: String, body: [String: Any]) async throws {
        var req = URLRequest(url: baseURL.appendingPathComponent(path))
        req.httpMethod = "POST"
        req.addValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONSerialization.data(withJSONObject: body)
        let (data, resp) = try await runRequest(req)
        try ensureOK(resp, data: data)
    }

    private func send<T: Decodable>(_ req: URLRequest) async throws -> T {
        let (data, resp) = try await runRequest(req)
        try ensureOK(resp, data: data)
        do {
            return try decoder.decode(T.self, from: data)
        } catch {
            throw CandleApiError.decoding(String(describing: error))
        }
    }

    private func runRequest(_ req: URLRequest) async throws -> (Data, URLResponse) {
        do {
            return try await session.data(for: req)
        } catch {
            throw CandleApiError.transport(error.localizedDescription)
        }
    }

    private func ensureOK(_ resp: URLResponse, data: Data) throws {
        guard let http = resp as? HTTPURLResponse else {
            throw CandleApiError.transport("No HTTP response")
        }
        if !(200..<300).contains(http.statusCode) {
            let body = String(data: data, encoding: .utf8) ?? ""
            throw CandleApiError.http(http.statusCode, body)
        }
    }
}
