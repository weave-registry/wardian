// The main program of Wardian.app (scripts/package-macos.sh). Wardian itself is a web server with no
// window; this launcher gives it what a Mac app needs: a Dock icon, Quit (which stops the server),
// opening Wardian in the browser, and a place for .wardian files opened from Finder to go.
//
// It starts Contents/MacOS/wardian with DATA_DIR in ~/Library/Application Support/Wardian (wardian
// finds the example apps in ../Resources/apps beside itself for the first start, ADR-2610080915), and
// logs the server's output to <data dir>/wardian.log. If a Wardian already answers on the address,
// it opens that one instead of starting a second.
import AppKit

final class Launcher: NSObject, NSApplicationDelegate {
    let env = ProcessInfo.processInfo.environment
    lazy var addr = env["ADDR"] ?? "127.0.0.1:8000"
    lazy var base = URL(string: "http://\(addr)/")!
    lazy var dataDir: URL = env["DATA_DIR"].map { URL(fileURLWithPath: $0) }
        ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("Wardian")
    var server: Process?
    var ready = false
    var pending: [URL] = []
    var quitting = false

    var signals: [DispatchSourceSignal] = []

    func applicationDidFinishLaunching(_ note: Notification) {
        buildMenu()
        // Stopped by a signal (kill, logout): stop the server too, as Quit does.
        for sig in [SIGTERM, SIGINT, SIGHUP] {
            signal(sig, SIG_IGN)
            let src = DispatchSource.makeSignalSource(signal: sig, queue: .main)
            src.setEventHandler { NSApp.terminate(nil) }
            src.resume()
            signals.append(src)
        }
        if answers() {
            becameReady()
            return
        }
        start()
    }

    func application(_ app: NSApplication, open urls: [URL]) {
        pending += urls
        if ready { showPending() }
    }

    func applicationShouldHandleReopen(_ app: NSApplication, hasVisibleWindows: Bool) -> Bool {
        if ready { NSWorkspace.shared.open(base) }
        return false
    }

    func applicationWillTerminate(_ note: Notification) {
        quitting = true
        server?.terminate()
        server?.waitUntilExit()
    }

    // MARK: the server

    func start() {
        guard let exe = Bundle.main.url(forAuxiliaryExecutable: "wardian") else {
            fatal("Wardian.app is missing its server program (Contents/MacOS/wardian).")
            return
        }
        try? FileManager.default.createDirectory(at: dataDir, withIntermediateDirectories: true)
        let logURL = dataDir.appendingPathComponent("wardian.log")
        if !FileManager.default.fileExists(atPath: logURL.path) {
            FileManager.default.createFile(atPath: logURL.path, contents: nil)
        }
        let log = try? FileHandle(forWritingTo: logURL)
        log?.seekToEndOfFile()
        log?.write("\n---- Wardian.app started the server at \(Date())\n".data(using: .utf8)!)

        let p = Process()
        p.executableURL = exe
        p.currentDirectoryURL = Bundle.main.resourceURL
        var e = env
        e["DATA_DIR"] = dataDir.path
        e["ADDR"] = addr
        p.environment = e
        p.standardOutput = log
        p.standardError = log
        p.terminationHandler = { [weak self] proc in
            DispatchQueue.main.async {
                guard let self, !self.quitting else { return }
                self.fatal("The Wardian server stopped (exit \(proc.terminationStatus)). Its log is \(logURL.path).")
            }
        }
        do {
            try p.run()
        } catch {
            fatal("Wardian could not start its server: \(error.localizedDescription)")
            return
        }
        server = p
        waitUntilReady(tries: 100)
    }

    func waitUntilReady(tries: Int) {
        if answers() { becameReady(); return }
        if tries == 0 {
            fatal("The Wardian server did not answer on \(addr). Its log is \(dataDir.appendingPathComponent("wardian.log").path).")
            return
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { self.waitUntilReady(tries: tries - 1) }
    }

    /// True when a Wardian answers /api/status on the address.
    func answers() -> Bool {
        var req = URLRequest(url: base.appendingPathComponent("api/status"))
        req.timeoutInterval = 0.5
        let done = DispatchSemaphore(value: 0)
        var ok = false
        URLSession.shared.dataTask(with: req) { data, resp, _ in
            ok = (resp as? HTTPURLResponse)?.statusCode == 200 && data.map { String(decoding: $0, as: UTF8.self).contains("\"source\"") } == true
            done.signal()
        }.resume()
        _ = done.wait(timeout: .now() + 1)
        return ok
    }

    func becameReady() {
        ready = true
        // WARDIAN_NO_BROWSER=1: start without opening a browser tab (for checking the bundle).
        if env["WARDIAN_NO_BROWSER"] != "1" { NSWorkspace.shared.open(base) }
        showPending()
    }

    /// A .wardian file opened from Finder. Installing takes the user's look at what it holds, which
    /// Wardian's Import shows, so the launcher opens Wardian and points at the file.
    func showPending() {
        let files = pending
        pending = []
        guard !files.isEmpty else { return }
        NSApp.activate(ignoringOtherApps: true)
        let a = NSAlert()
        a.messageText = files.count == 1 ? "Install \(files[0].lastPathComponent)?" : "Install \(files.count) apps?"
        a.informativeText = "Wardian is open in your browser. Choose Import there and pick the file: Wardian shows what it holds before installing anything. The file is selected in Finder."
        a.addButton(withTitle: "OK")
        a.runModal()
        NSWorkspace.shared.activateFileViewerSelecting(files)
    }

    func fatal(_ message: String) {
        NSApp.activate(ignoringOtherApps: true)
        let a = NSAlert()
        a.alertStyle = .critical
        a.messageText = "Wardian"
        a.informativeText = message
        a.runModal()
        NSApp.terminate(nil)
    }

    // MARK: the menu

    @objc func openInBrowser(_ sender: Any?) { NSWorkspace.shared.open(base) }
    @objc func openLog(_ sender: Any?) { NSWorkspace.shared.open(dataDir.appendingPathComponent("wardian.log")) }
    @objc func openDataFolder(_ sender: Any?) { NSWorkspace.shared.open(dataDir) }

    func buildMenu() {
        let main = NSMenu()
        let appItem = NSMenuItem()
        main.addItem(appItem)
        let m = NSMenu()
        m.addItem(withTitle: "Open Wardian in the Browser", action: #selector(openInBrowser(_:)), keyEquivalent: "o")
        m.addItem(withTitle: "Show the Server Log", action: #selector(openLog(_:)), keyEquivalent: "l")
        m.addItem(withTitle: "Show the Data Folder", action: #selector(openDataFolder(_:)), keyEquivalent: "")
        m.addItem(.separator())
        m.addItem(withTitle: "Quit Wardian", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = m
        NSApp.mainMenu = main
    }
}

let app = NSApplication.shared
let delegate = Launcher()
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
