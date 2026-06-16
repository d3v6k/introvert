package chat.introvert.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import androidx.core.app.NotificationCompat

class IntrovertService : Service() {
    private val CHANNEL_ID = "introvert_background"
    private val NOTIFICATION_ID = 1001
    private var wakeLock: PowerManager.WakeLock? = null
    private var isForeground = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
        callStartForeground()
    }

    override fun onDestroy() {
        releaseWakeLock()
        super.onDestroy()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        callStartForeground()

        val shouldStayAwake = intent?.getBooleanExtra("awake", false) ?: false
        if (shouldStayAwake) {
            acquireWakeLock()
        } else {
            releaseWakeLock()
        }

        return START_STICKY
    }

    private fun callStartForeground() {
        if (isForeground) return
        try {
            val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
            val pendingIntent = PendingIntent.getActivity(
                this, 0, launchIntent,
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
            )

            val notification = NotificationCompat.Builder(this, CHANNEL_ID)
                .setContentTitle("Introvert P2P Active")
                .setContentText("Sovereign link is maintaining mesh connectivity.")
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentIntent(pendingIntent)
                .setPriority(NotificationCompat.PRIORITY_LOW)
                .build()

            startForeground(NOTIFICATION_ID, notification)
            isForeground = true
        } catch (e: Exception) {
            Log.e("IntrovertService", "startForeground failed: ${e.message}", e)
            try {
                startForeground(NOTIFICATION_ID, NotificationCompat.Builder(this, CHANNEL_ID)
                    .setContentTitle("Introvert P2P Active")
                    .setSmallIcon(android.R.drawable.ic_dialog_info)
                    .build())
                isForeground = true
            } catch (e2: Exception) {
                Log.e("IntrovertService", "Fallback startForeground also failed: ${e2.message}", e2)
            }
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
