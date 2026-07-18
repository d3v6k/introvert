package chat.introvert.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.net.wifi.WifiManager
import android.content.Intent
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import android.content.pm.ServiceInfo
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat

class IntrovertService : Service() {
    private val CHANNEL_ID = "introvert_background"
    private val NOTIFICATION_ID = 1001
    private var wakeLock: PowerManager.WakeLock? = null
    private var multicastLock: WifiManager.MulticastLock? = null

    companion object {
        // Static callback set by MainActivity when Flutter engine is ready.
        // Used to invoke MethodChannel calls from the service.
        var onWakeupCallback: (() -> Unit)? = null
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
    }

    override fun onDestroy() {
        releaseMulticastLock()
        releaseWakeLock()
        super.onDestroy()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // CRITICAL: Call startForeground IMMEDIATELY — before anything else.
        // Android kills the app if this isn't called within 5 seconds of startForegroundService().
        startForegroundMinimal()
        acquireMulticastLock()

        val shouldStayAwake = intent?.getBooleanExtra("awake", false) ?: false
        if (shouldStayAwake) {
            acquireWakeLock()
        } else {
            releaseWakeLock()
        }

        // Check for pending wakeup from FCM push — trigger mailbox fetch via Flutter
        val prefs = getSharedPreferences("introvert_fcm", Context.MODE_PRIVATE)
        if (prefs.getBoolean("pending_wakeup", false)) {
            prefs.edit().putBoolean("pending_wakeup", false).apply()
            Log.d("IntrovertService", "Pending wakeup detected — invoking onWakeup callback")
            try {
                onWakeupCallback?.invoke()
            } catch (e: Exception) {
                Log.e("IntrovertService", "onWakeup callback failed: ${e.message}")
            }
        }

        return START_STICKY
    }

    private fun startForegroundMinimal() {
        // Always create channel first (idempotent, fast)
        createNotificationChannel()

        try {
            val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
            val pendingIntent = if (launchIntent != null) {
                PendingIntent.getActivity(
                    this, 0, launchIntent,
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
                )
            } else null

            val builder = NotificationCompat.Builder(this, CHANNEL_ID)
                .setContentTitle("Introvert P2P Active")
                .setContentText("Sovereign link is maintaining mesh connectivity.")
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setPriority(NotificationCompat.PRIORITY_LOW)
                .setOngoing(true)

            if (pendingIntent != null) {
                builder.setContentIntent(pendingIntent)
            }

            startForegroundCompat(NOTIFICATION_ID, builder.build())
        } catch (e: Exception) {
            Log.e("IntrovertService", "startForeground failed: ${e.message}", e)
            // Last resort: try with absolute minimal notification
            try {
                val fallback = Notification.Builder(this, CHANNEL_ID)
                    .setContentTitle("Introvert")
                    .setSmallIcon(android.R.drawable.ic_dialog_info)
                    .build()
                startForegroundCompat(NOTIFICATION_ID, fallback)
            } catch (e2: Exception) {
                Log.e("IntrovertService", "Fallback startForeground also failed: ${e2.message}", e2)
            }
        }
    }

    private fun startForegroundCompat(id: Int, notification: android.app.Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ServiceCompat.startForeground(
                this,
                id,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
            )
        } else {
            startForeground(id, notification)
        }
    }

    private fun acquireWakeLock() {
        if (wakeLock == null) {
            val powerManager = getSystemService(Context.POWER_SERVICE) as PowerManager
            wakeLock = powerManager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Introvert::NodeWakeLock").apply {
                acquire()
            }
            Log.d("IntrovertService", "Node Mode: WakeLock acquired.")
        }
    }

    private fun releaseWakeLock() {
        wakeLock?.let {
            if (it.isHeld) {
                it.release()
            }
        }
        wakeLock = null
        Log.d("IntrovertService", "Standard Mode: WakeLock released.")
    }

    private fun acquireMulticastLock() {
        if (multicastLock == null) {
            val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            multicastLock = wifi.createMulticastLock("introvert_mdns").apply {
                setReferenceCounted(true)
                acquire()
            }
            Log.d("IntrovertService", "MulticastLock acquired for mDNS discovery.")
        }
    }

    private fun releaseMulticastLock() {
        multicastLock?.let {
            if (it.isHeld) {
                it.release()
            }
        }
        multicastLock = null
        Log.d("IntrovertService", "MulticastLock released.")
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val serviceChannel = NotificationChannel(
                CHANNEL_ID,
                "Introvert Background Service",
                NotificationManager.IMPORTANCE_LOW
            )
            val manager = getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(serviceChannel)
        }
    }
}
