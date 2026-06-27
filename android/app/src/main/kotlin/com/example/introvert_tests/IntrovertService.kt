package com.example.introvert_tests

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat

class IntrovertService : Service() {
    private val CHANNEL_ID = "introvert_background"
    private val NOTIFICATION_ID = 1001
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onDestroy() {
        releaseWakeLock()
        super.onDestroy()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
        val pendingIntent = PendingIntent.getActivity(
            this, 0, launchIntent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        val notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Introvert P2P Active")
            .setContentText("Sovereign link is maintaining mesh connectivity.")
            .setSmallIcon(android.R.drawable.ic_dialog_info) // Fallback icon
            .setContentIntent(pendingIntent)
            .setPriority(NotificationCompat.PRIORITY_LOW) // Use LOW to avoid intruding
            .build()

        startForeground(NOTIFICATION_ID, notification)

        // Handle WakeLock toggle for Node Mode
        val shouldStayAwake = intent?.getBooleanExtra("awake", false) ?: false
        if (shouldStayAwake) {
            acquireWakeLock()
        } else {
            releaseWakeLock()
        }

        return START_STICKY
    }

    private fun acquireWakeLock() {
        if (wakeLock == null) {
            val powerManager = getSystemService(Context.POWER_SERVICE) as PowerManager
            wakeLock = powerManager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Introvert::NodeWakeLock").apply {
                acquire()
            }
            android.util.Log.d("IntrovertService", "✅ Node Mode: WakeLock acquired.")
        }
    }

    private fun releaseWakeLock() {
        wakeLock?.let {
            if (it.isHeld) {
                it.release()
            }
        }
        wakeLock = null
        android.util.Log.d("IntrovertService", "🔋 Standard Mode: WakeLock released.")
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
