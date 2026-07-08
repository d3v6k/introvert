package chat.introvert.app

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import chat.introvert.app.R
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import com.google.firebase.messaging.FirebaseMessaging

class MainActivity : FlutterActivity() {
    private val CHANNEL = "introvert/alerts"
    private val MSG_CHANNEL_ID = "introvert_messages"
    private val CALL_CHANNEL_ID = "introvert_calls"
    private val NOTIF_PERMISSION_CODE = 101

    // Keep a reference so we can release it properly
    private var mediaPlayer: MediaPlayer? = null
    private var audioFocusRequest: AudioFocusRequest? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        createNotificationChannels()
        requestNotificationPermission()
        requestBatteryOptimizationExemption()
        handlePushIntent(intent)
        
        try {
            FirebaseMessaging.getInstance().token.addOnCompleteListener { task ->
                if (task.isSuccessful && task.result != null) {
                    val token = task.result
                    Log.d("IntrovertFCM", "Launch FCM token: ${token.take(20)}...")
                    val prefs = getSharedPreferences("introvert_fcm", Context.MODE_PRIVATE)
                    prefs.edit().putString("pending_fcm_token", token).apply()
                    forwardPendingFcmToken()
                } else {
                    Log.w("IntrovertFCM", "Failed to retrieve FCM token: ${task.exception?.message}")
                }
            }
        } catch (e: Exception) {
            Log.e("IntrovertFCM", "Error fetching FCM token: ${e.message}", e)
        }

        forwardPendingFcmToken()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handlePushIntent(intent)
    }

    override fun onResume() {
        super.onResume()
        IntrovertFirebaseMessagingService.isAppInForeground = true
        forwardPendingFcmToken()
        forwardPendingWakeup()
    }

    override fun onPause() {
        super.onPause()
        IntrovertFirebaseMessagingService.isAppInForeground = false
    }

    /**
     * Check SharedPreferences for a pending FCM token saved by IntrovertFirebaseMessagingService
     * and forward it to Flutter. Clears the token after forwarding.
     */
    private fun forwardPendingFcmToken() {
        val prefs = getSharedPreferences("introvert_fcm", Context.MODE_PRIVATE)
        val pendingToken = prefs.getString("pending_fcm_token", null) ?: return

        Log.d("IntrovertFCM", "Forwarding pending FCM token to Flutter")
        val flutterEngine = flutterEngine ?: return
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL)
            .invokeMethod("onPushNotification", hashMapOf("fcm_token" to pendingToken))

        prefs.edit().remove("pending_fcm_token").apply()
    }

    /**
     * Check for a pending wakeup flag (set by FCM service) and trigger mailbox fetch.
     */
    private fun forwardPendingWakeup() {
        val prefs = getSharedPreferences("introvert_fcm", Context.MODE_PRIVATE)
        if (!prefs.getBoolean("pending_wakeup", false)) return

        prefs.edit().putBoolean("pending_wakeup", false).apply()
        Log.d("IntrovertFCM", "Forwarding pending wakeup to Flutter")
        try {
            val flutterEngine = flutterEngine ?: return
            MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL)
                .invokeMethod("onWakeup", null)
        } catch (e: Exception) {
            Log.w("IntrovertFCM", "Failed to forward wakeup: ${e.message}")
        }
    }

    /**
     * Handle push notification intents from FirebaseMessagingService.
     * These intents contain "open_chat", "open_group", or "fcm_token" extras.
     */
    private fun handlePushIntent(intent: Intent?) {
        intent?.let {
            val openChat = it.getStringExtra("open_chat")
            val openGroup = it.getStringExtra("open_group")
            val incomingCall = it.getStringExtra("incoming_call")
            val fcmToken = it.getStringExtra("fcm_token")
            
            if (openChat != null || openGroup != null || incomingCall != null || fcmToken != null) {
                val flutterEngine = flutterEngine ?: return
                val args = HashMap<String, String?>()
                args["open_chat"] = openChat
                args["open_group"] = openGroup
                args["incoming_call"] = incomingCall
                args["fcm_token"] = fcmToken
                MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL)
                    .invokeMethod("onPushNotification", args)
            }
        }
    }

    override fun onDestroy() {
        releaseMediaPlayer()
        super.onDestroy()
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)

        // Wire IntrovertService wakeup to Flutter MethodChannel
        IntrovertService.onWakeupCallback = {
            MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL)
                .invokeMethod("onWakeup", null)
        }

        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "showAlert" -> {
                        val title  = call.argument<String>("title")  ?: "Introvert"
                        val body   = call.argument<String>("body")   ?: ""
                        val isCall = call.argument<Boolean>("isCall") ?: false
                        // Run on main thread to be safe
                        Handler(Looper.getMainLooper()).post {
                            showNotification(title, body, isCall)
                        }
                        result.success(null)
                    }
                    "requestPermissions" -> {
                        requestNotificationPermission()
                        result.success(null)
                    }
                    "startBackgroundService" -> {
                        val awake = call.argument<Boolean>("awake") ?: false
                        startIntrovertService(awake)
                        result.success(null)
                    }
                    "stopBackgroundService" -> {
                        stopIntrovertService()
                        result.success(null)
                    }
                    "setStayAwake" -> {
                        val awake = call.argument<Boolean>("awake") ?: false
                        updateServiceWakeLock(awake)
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
    }

    private fun updateServiceWakeLock(awake: Boolean) {
        val intent = Intent(this, IntrovertService::class.java).apply {
            putExtra("awake", awake)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            startService(intent)
        }
    }

    private fun startIntrovertService(awake: Boolean) {
        val intent = Intent(this, IntrovertService::class.java).apply {
            putExtra("awake", awake)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            startService(intent)
        }
    }

    private fun stopIntrovertService() {
        val intent = Intent(this, IntrovertService::class.java)
        stopService(intent)
    }


    // ─── Notification Channels ────────────────────────────────────────────────

    private fun createNotificationChannels() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return

        val notificationManager =
            getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

        // Sound URI pointing to res/raw/introvert_ping.m4a
        val soundUri = Uri.parse(
            "android.resource://${packageName}/${R.raw.introvert_ping}"
        )
        val audioAttributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_NOTIFICATION)
            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
            .build()

        // Message channel
        val msgChannel = NotificationChannel(
            MSG_CHANNEL_ID,
            "Introvert Messages",
            NotificationManager.IMPORTANCE_HIGH
        ).apply {
            description = "Notifications for incoming Introvert messages"
            enableLights(true)
            enableVibration(true)
            vibrationPattern = longArrayOf(0, 80, 60, 80)
            setShowBadge(true)
            lockscreenVisibility = android.app.Notification.VISIBILITY_PRIVATE
            setSound(soundUri, audioAttributes)
        }

        // Call channel (critical importance — can override DND)
        val callChannel = NotificationChannel(
            CALL_CHANNEL_ID,
            "Introvert Calls",
            NotificationManager.IMPORTANCE_HIGH
        ).apply {
            description = "Notifications for incoming Introvert calls"
            enableLights(true)
            enableVibration(true)
            vibrationPattern = longArrayOf(0, 500, 200, 500)
            setShowBadge(true)
            lockscreenVisibility = android.app.Notification.VISIBILITY_PUBLIC
            setSound(soundUri, audioAttributes)
        }

        notificationManager.createNotificationChannel(msgChannel)
        notificationManager.createNotificationChannel(callChannel)
    }

    // ─── Show Notification ────────────────────────────────────────────────────

    private fun showNotification(title: String, body: String, isCall: Boolean) {
        android.util.Log.d("IntrovertAlert", "Showing notification: $title - $body")
        val notificationManager =
            applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

        // Tap-to-open intent — brings the app to the foreground
        val launchIntent = packageManager.getLaunchIntentForPackage(packageName)?.apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        val pendingIntent = PendingIntent.getActivity(
            applicationContext, 0, launchIntent ?: intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        val channelId = if (isCall) CALL_CHANNEL_ID else MSG_CHANNEL_ID

        // Resolve launcher icon
        var smallIconRes = applicationContext.resources.getIdentifier("launcher_icon", "mipmap", packageName)
        if (smallIconRes == 0) {
            smallIconRes = applicationContext.resources.getIdentifier("ic_launcher", "mipmap", packageName)
        }
        if (smallIconRes == 0) smallIconRes = android.R.drawable.ic_dialog_info

        val notification = NotificationCompat.Builder(applicationContext, channelId)
            .setSmallIcon(smallIconRes)
            .setContentTitle(title)
            .setContentText(body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setCategory(
                if (isCall) NotificationCompat.CATEGORY_CALL
                else NotificationCompat.CATEGORY_MESSAGE
            )
            .setAutoCancel(true)
            .setContentIntent(pendingIntent)
            .setDefaults(
                if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O)
                    NotificationCompat.DEFAULT_ALL
                else
                    NotificationCompat.DEFAULT_VIBRATE or NotificationCompat.DEFAULT_LIGHTS
            )
            .build()

        notificationManager.notify(System.currentTimeMillis().toInt(), notification)
        playPingSound()
    }

    // ─── In-process Ping Sound ────────────────────────────────────────────────

    private fun playPingSound() {
        try {
            releaseMediaPlayer()

            val audioManager = getSystemService(Context.AUDIO_SERVICE) as AudioManager

            // Request transient audio focus so we don't interrupt music indefinitely
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                val focusRequest = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK)
                    .setAudioAttributes(
                        AudioAttributes.Builder()
                            .setUsage(AudioAttributes.USAGE_NOTIFICATION)
                            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                            .build()
                    )
                    .setAcceptsDelayedFocusGain(false)
                    .build()
                audioFocusRequest = focusRequest
                audioManager.requestAudioFocus(focusRequest)
            } else {
                @Suppress("DEPRECATION")
                audioManager.requestAudioFocus(
                    null, AudioManager.STREAM_NOTIFICATION, AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK
                )
            }

            val mp = MediaPlayer()
            mediaPlayer = mp

            // Prefer res/raw (bundled as native resource, works without Flutter asset loader)
            val rawResId = resources.getIdentifier("introvert_ping", "raw", packageName)
            if (rawResId != 0) {
                val afd = resources.openRawResourceFd(rawResId)
                mp.setDataSource(afd.fileDescriptor, afd.startOffset, afd.length)
                afd.close()
            } else {
                // Fallback: flutter_assets path inside the APK
                val assetFd = assets.openFd("flutter_assets/assets/audio/introvert_ping.m4a")
                mp.setDataSource(assetFd.fileDescriptor, assetFd.startOffset, assetFd.length)
                assetFd.close()
            }

            mp.setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_NOTIFICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                    .build()
            )
            mp.setOnCompletionListener { player ->
                player.release()
                mediaPlayer = null
                // Release audio focus
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    audioFocusRequest?.let { audioManager.abandonAudioFocusRequest(it) }
                } else {
                    @Suppress("DEPRECATION")
                    audioManager.abandonAudioFocus(null)
                }
            }
            mp.prepareAsync()
            mp.setOnPreparedListener { it.start() }
        } catch (e: Exception) {
            android.util.Log.e("IntrovertPing", "Failed to play ping sound: ${e.message}", e)
        }
    }

    private fun releaseMediaPlayer() {
        try {
            mediaPlayer?.let {
                if (it.isPlaying) it.stop()
                it.release()
            }
        } catch (_: Exception) {}
        mediaPlayer = null
    }

    // ─── Permissions ──────────────────────────────────────────────────────────

    /**
     * Request exemption from battery optimization (Doze mode) so the foreground service
     * and FCM wake-ups are not throttled when the screen is off.
     * This is critical for P2P mesh connectivity — without it, Android can freeze
     * the Tokio runtime and the 15-second status check stops.
     */
    private fun requestBatteryOptimizationExemption() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            val pm = getSystemService(Context.POWER_SERVICE) as android.os.PowerManager
            if (!pm.isIgnoringBatteryOptimizations(packageName)) {
                try {
                    val intent = Intent(android.provider.Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS).apply {
                        data = android.net.Uri.parse("package:$packageName")
                    }
                    startActivity(intent)
                    Log.d("MainActivity", "Requested battery optimization exemption")
                } catch (e: Exception) {
                    Log.w("MainActivity", "Could not request battery optimization exemption: ${e.message}")
                }
            }
        }
    }

    private fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (ContextCompat.checkSelfPermission(
                    this, Manifest.permission.POST_NOTIFICATIONS
                ) != PackageManager.PERMISSION_GRANTED
            ) {
                ActivityCompat.requestPermissions(
                    this,
                    arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                    NOTIF_PERMISSION_CODE
                )
            }
        }
    }
}
