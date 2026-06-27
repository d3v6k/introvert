import Flutter
import UIKit
import UserNotifications
import AVFoundation

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {

    // Strong reference so the player isn't deallocated mid-playback
    private var audioPlayer: AVAudioPlayer?

    override func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        let result = super.application(application, didFinishLaunchingWithOptions: launchOptions)

        // ── UNUserNotificationCenter ──────────────────────────────────────────
        UNUserNotificationCenter.current().delegate = self
        UNUserNotificationCenter.current().requestAuthorization(
            options: [.alert, .sound, .badge]
        ) { granted, error in
            if let error = error {
                print("🔔 AppDelegate: Notification permission error: \(error)")
            } else {
                print("🔔 AppDelegate: Notification permission granted: \(granted)")
                if granted {
                    DispatchQueue.main.async {
                        application.registerForRemoteNotifications()
                    }
                }
            }
        }

        // ── AVAudioSession ────────────────────────────────────────────────────
        // .playback + .mixWithOthers: play ping while backgrounded without
        // interrupting the user's music.
        configureAudioSession()

        // Ensure the ping is copied to Library/Sounds/ once at launch so
        // UNNotificationSound can find it even when the app is fully suspended.
        ensureNotificationSound()

        return result
    }

    // ── APNs Device Token ───────────────────────────────────────────────────

    override func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        let tokenParts = deviceToken.map { data in String(format: "%02.2hhx", data) }
        let token = tokenParts.joined()
        print("🔔 AppDelegate: APNs Device Token: \(token)")

        // Pass to Flutter so the app can register this token with the RBN
        if let controller = window?.rootViewController as? FlutterViewController {
            let channel = FlutterMethodChannel(name: "introvert/alerts", binaryMessenger: controller.binaryMessenger)
            channel.invokeMethod("onDeviceToken", arguments: token)
        }
    }

    override func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        print("🔔 AppDelegate: Failed to register for remote notifications: \(error)")
    }

    // ── Silent Push Wakeup (WhatsApp-style) ──────────────────────────────────
    // This wakes the app for 30 seconds when the RBN sends a "Ping".

    override func application(
        _ application: UIApplication,
        didReceiveRemoteNotification userInfo: [AnyHashable : Any],
        fetchCompletionHandler completionHandler: @escaping (UIBackgroundFetchResult) -> Void
    ) {
        print("🔔 AppDelegate: Received Background Remote Notification")

        // Trigger Flutter to perform a P2P Mailbox Fetch
        if let controller = window?.rootViewController as? FlutterViewController {
            let channel = FlutterMethodChannel(name: "introvert/alerts", binaryMessenger: controller.binaryMessenger)
            channel.invokeMethod("onWakeup", arguments: nil)
        }

        // Allow the app some time to fetch data before suspending again
        DispatchQueue.main.asyncAfter(deadline: .now() + 15.0) {
            completionHandler(.newData)
        }
    }

    // ── FlutterImplicitEngineDelegate ─────────────────────────────────────────
    // This is the modern, non-deprecated way to register platform channels.
    // It runs after the Flutter engine is fully initialised.
    func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
        GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)

        // registrar(forPlugin:) returns an optional — guard-unwrap it.
        guard let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "IntrovertAlerts") else {
            print("🔔 AppDelegate: Could not get plugin registrar — channels not registered")
            return
        }

        let alertChannel = FlutterMethodChannel(
            name: "introvert/alerts",
            binaryMessenger: registrar.messenger()
        )

        // Explicit type annotations are required here so Swift can resolve the
        // overloaded setMethodCallHandler without a type annotation error.
        alertChannel.setMethodCallHandler {
            [weak self] (call: FlutterMethodCall, result: @escaping FlutterResult) in
            switch call.method {
            case "showAlert":
                guard
                    let args   = call.arguments as? [String: Any],
                    let title  = args["title"]   as? String,
                    let body   = args["body"]    as? String,
                    let isCall = args["isCall"]  as? Bool
                else {
                    result(FlutterError(code: "INVALID_ARGUMENTS",
                                        message: "Missing or malformed arguments",
                                        details: nil))
                    return
                }
                self?.deliverAlert(title: title, body: body, isCall: isCall)
                result(nil)

            case "requestPermissions":
                UNUserNotificationCenter.current().requestAuthorization(
                    options: [.alert, .sound, .badge]
                ) { _, _ in }
                result(nil)

            default:
                result(FlutterMethodNotImplemented)
            }
        }
    }

    // ── Foreground presentation ───────────────────────────────────────────────
    override func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        if #available(iOS 14.0, *) {
            completionHandler([.banner, .sound, .badge])
        } else {
            completionHandler([.alert, .sound, .badge])
        }
    }

    // ── Tap handler ───────────────────────────────────────────────────────────
    override func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        completionHandler()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // MARK: - Private helpers
    // ─────────────────────────────────────────────────────────────────────────

    private func deliverAlert(title: String, body: String, isCall: Bool) {
        postLocalNotification(title: title, body: body, isCall: isCall)
        playPingInProcess()
    }

    // ── Local notification ────────────────────────────────────────────────────

    private func postLocalNotification(title: String, body: String, isCall: Bool) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body  = body
        content.badge = 1

        // Attach the custom sound (copied to Library/Sounds/ at launch).
        // Falls back to system default if the copy failed.
        let filename = "introvert_ping.m4a"
        let soundsURL = FileManager.default
            .urls(for: .libraryDirectory, in: .userDomainMask)
            .first?
            .appendingPathComponent("Sounds/\(filename)")

        if let url = soundsURL, FileManager.default.fileExists(atPath: url.path) {
            content.sound = UNNotificationSound(named: UNNotificationSoundName(rawValue: filename))
        } else {
            content.sound = .default
        }

        // Give call notifications higher visual priority (iOS 15+)
        if isCall, #available(iOS 15.0, *) {
            content.interruptionLevel = .timeSensitive
        }

        let trigger = UNTimeIntervalNotificationTrigger(timeInterval: 0.1, repeats: false)
        let request = UNNotificationRequest(
            identifier: "introvert_\(UUID().uuidString)",
            content: content,
            trigger: trigger
        )
        UNUserNotificationCenter.current().add(request) { error in
            if let error = error {
                print("🔔 AppDelegate: Failed to post notification: \(error)")
            }
        }
    }

    // ── Copy ping asset to Library/Sounds/ ───────────────────────────────────
    // The system only picks up custom notification sounds from this directory.

    @discardableResult
    private func ensureNotificationSound() -> Bool {
        let filename = "introvert_ping.m4a"
        let fm = FileManager.default
        guard let libraryURL = fm.urls(for: .libraryDirectory, in: .userDomainMask).first else {
            return false
        }
        let soundsDir = libraryURL.appendingPathComponent("Sounds", isDirectory: true)
        let destURL   = soundsDir.appendingPathComponent(filename)

        guard !fm.fileExists(atPath: destURL.path) else { return true }

        try? fm.createDirectory(at: soundsDir, withIntermediateDirectories: true)

        guard let sourceURL = locatePingAsset() else {
            print("🔔 AppDelegate: Ping asset not found in bundle")
            return false
        }
        do {
            try fm.copyItem(at: sourceURL, to: destURL)
            print("🔔 AppDelegate: Copied ping sound to Library/Sounds/")
            return true
        } catch {
            print("🔔 AppDelegate: Failed to copy ping sound: \(error)")
            return false
        }
    }

    private func locatePingAsset() -> URL? {
        let bundle = Bundle.main
        let assetKey = "assets/audio/introvert_ping.m4a"

        // 1. Frameworks/App.framework/flutter_assets  (standard device build)
        let frameworkPath = bundle.bundlePath
            + "/Frameworks/App.framework/flutter_assets/" + assetKey
        if FileManager.default.fileExists(atPath: frameworkPath) {
            return URL(fileURLWithPath: frameworkPath)
        }

        // 2. Root flutter_assets  (simulator / some archive layouts)
        if let url = bundle.url(
            forResource: assetKey, withExtension: nil,
            subdirectory: "flutter_assets"
        ) { return url }

        // 3. Recursive bundle search as last resort
        if let resourcePath = bundle.resourcePath {
            let enumerator = FileManager.default.enumerator(atPath: resourcePath)
            while let file = enumerator?.nextObject() as? String {
                if file.hasSuffix("introvert_ping.m4a") {
                    return URL(fileURLWithPath: resourcePath)
                        .appendingPathComponent(file)
                }
            }
        }
        return nil
    }

    // ── AVAudioSession ────────────────────────────────────────────────────────

    private func configureAudioSession() {
        do {
            try AVAudioSession.sharedInstance().setCategory(
                .playback,
                mode: .default,
                options: [.mixWithOthers, .duckOthers]
            )
            try AVAudioSession.sharedInstance().setActive(true)
        } catch {
            print("🔔 AppDelegate: AVAudioSession config error: \(error)")
        }
    }

    private func playPingInProcess() {
        guard let url = locatePingAsset() else {
            print("🔔 AppDelegate: playPingInProcess — asset not found")
            return
        }
        do {
            try AVAudioSession.sharedInstance().setActive(true)
            let player = try AVAudioPlayer(contentsOf: url)
            player.numberOfLoops = 0
            player.volume = 1.0
            player.prepareToPlay()
            player.play()
            audioPlayer = player
        } catch {
            print("🔔 AppDelegate: AVAudioPlayer error: \(error)")
        }
    }
}
