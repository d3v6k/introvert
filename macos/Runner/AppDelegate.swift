import Cocoa
import FlutterMacOS
import UserNotifications
import AppKit

@main
class AppDelegate: FlutterAppDelegate {
  private var soundPlayer: NSSound?

  override func applicationDidFinishLaunching(_ notification: Notification) {
    let controller = mainFlutterWindow?.contentViewController as! FlutterViewController
    let alertChannel = FlutterMethodChannel(name: "introvert/alerts",
                                            binaryMessenger: controller.engine.binaryMessenger)
    
    alertChannel.setMethodCallHandler({
      [weak self] (call: FlutterMethodCall, result: @escaping FlutterResult) -> Void in
      if call.method == "showAlert" {
        guard let args = call.arguments as? [String: Any],
              let title = args["title"] as? String,
              let body = args["body"] as? String,
              let isCall = args["isCall"] as? Bool else {
          result(FlutterError(code: "INVALID_ARGUMENTS", message: "Arguments were invalid", details: nil))
          return
        }
        self?.showNotification(title: title, body: body, isCall: isCall)
        result(nil)
      } else {
        result(FlutterMethodNotImplemented)
      }
    })

    // Request local notification permissions
    UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
        if let error = error {
            print("🔔 AppDelegate: Notification permission error: \(error)")
        } else {
            print("🔔 AppDelegate: Notification permission granted: \(granted)")
        }
    }
    
    UNUserNotificationCenter.current().delegate = self
  }

  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return true
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }

  private func showNotification(title: String, body: String, isCall: Bool) {
    let content = UNMutableNotificationContent()
    content.title = title
    content.body = body
    
    // Play the custom ping sound
    playPingSound()
    
    let trigger = UNTimeIntervalNotificationTrigger(timeInterval: 0.1, repeats: false)
    let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: trigger)
    UNUserNotificationCenter.current().add(request) { error in
      if let error = error {
        print("🔔 AppDelegate: Error showing notification: \(error)")
      }
    }
  }

  private func playPingSound() {
    // 1. Try local absolute path first (from requirements: /Users/dev/Documents/introvert logo/introvert_ping.m4a)
    let devPath = "/Users/dev/Documents/introvert logo/introvert_ping.m4a"
    if FileManager.default.fileExists(atPath: devPath) {
        if let sound = NSSound(contentsOfFile: devPath, byReference: true) {
            soundPlayer = sound
            sound.play()
            return
        }
    }
    
    // 2. Try App Bundle assets
    let key = "assets/audio/introvert_ping.m4a"
    if let path = Bundle.main.path(forResource: key, ofType: nil, inDirectory: "Contents/Frameworks/App.framework/Resources/flutter_assets") {
        if let sound = NSSound(contentsOfFile: path, byReference: true) {
            soundPlayer = sound
            sound.play()
            return
        }
    }
    
    // 3. Search bundle recursively
    let fm = FileManager.default
    if let resourcePath = Bundle.main.resourcePath {
        let enumerator = fm.enumerator(atPath: resourcePath)
        while let file = enumerator?.nextObject() as? String {
            if file.hasSuffix("introvert_ping.m4a") {
                let fullPath = URL(fileURLWithPath: resourcePath).appendingPathComponent(file).path
                if let sound = NSSound(contentsOfFile: fullPath, byReference: true) {
                    soundPlayer = sound
                    sound.play()
                    return
                }
            }
        }
    }
    print("🔔 AppDelegate: Ping sound asset not found on macOS")
  }
}

// Conforming to UNUserNotificationCenterDelegate to display notifications when active or in bg
extension AppDelegate: UNUserNotificationCenterDelegate {
  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    completionHandler([.alert, .sound, .badge])
  }
}
