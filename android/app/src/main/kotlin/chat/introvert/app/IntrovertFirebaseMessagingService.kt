package chat.introvert.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage

/**
 * Handles FCM push notifications for Introvert.
 * 
 * Flow:
 * 1. App registers FCM token with RBN via IdentifySleepState
 * 2. When message stored in mailbox, RBN sends FCM push
 * 3. This service receives the push and wakes the app
 * 4. App fetches actual encrypted message from RBN mailbox
 */
class IntrovertFirebaseMessagingService : FirebaseMessagingService() {

    companion object {
        private const val TAG = "IntrovertFCM"
        private const val MSG_CHANNEL = "introvert_messages"
        private const val CALL_CHANNEL = "introvert_calls"
        private const val NOTIFICATION_ID_BASE = 2000
    }

    /**
     * Called when a new FCM registration token is generated.
     * Persists the token to SharedPreferences and forwards it to MainActivity
     * for registration with the RBN.
     */
    override fun onNewToken(token: String) {
        super.onNewToken(token)
        Log.d(TAG, "New FCM token received: ${token.take(20)}...")

        // Persist the token so MainActivity can read it on next resume
        val prefs = getSharedPreferences("introvert_fcm", Context.MODE_PRIVATE)
        prefs.edit().putString("pending_fcm_token", token).apply()

        // Try to forward token to MainActivity (may not work if app is killed on some OEMs)
        try {
            val intent = Intent(this, MainActivity::class.java).apply {
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
                putExtra("fcm_token", token)
            }
            startActivity(intent)
        } catch (e: Exception) {
            Log.w(TAG, "Could not launch MainActivity for token: ${e.message}")
        }
    }

    /**
     * Called when a push notification is received.
     * 
     * Payload from RBN contains only:
     * - sender_peer_id: Who sent the message
     * - message_type: "chat" | "call" | "group"
     * 
     * Actual message content is NOT in the push (stays encrypted in mailbox).
     */
    override fun onMessageReceived(message: RemoteMessage) {
        super.onMessageReceived(message)
        Log.d(TAG, "Push notification received: ${message.data}")
        
        val data = message.data
        val senderPeerId = data["sender_peer_id"] ?: ""
        val messageType = data["message_type"] ?: "chat"
        
        // Wake up the foreground service to fetch messages
        wakeForegroundService()
        
        // Show local notification
        when (messageType) {
            "call" -> showCallNotification(senderPeerId)
            "group" -> showGroupNotification(senderPeerId, data)
            else -> showMessageNotification(senderPeerId)
        }
    }

    /**
     * Wake up the IntrovertService to fetch pending messages from RBN.
     */
    private fun wakeForegroundService() {
        try {
            val intent = Intent(this, IntrovertService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                startForegroundService(intent)
            } else {
                startService(intent)
            }
            Log.d(TAG, "Foreground service started for message fetch")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start foreground service: ${e.message}")
        }
    }

    /**
     * Show notification for incoming message.
     */
    private fun showMessageNotification(senderPeerId: String) {
        val notificationManager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        createNotificationChannel(notificationManager, MSG_CHANNEL, "Messages", "New message notifications")
        
        val intent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            putExtra("open_chat", senderPeerId)
        }
        val pendingIntent = PendingIntent.getActivity(
            this, NOTIFICATION_ID_BASE, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

        val notification = NotificationCompat.Builder(this, MSG_CHANNEL)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("New Message")
            .setContentText("You have a new message")
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setAutoCancel(true)
            .setContentIntent(pendingIntent)
            .build()

        notificationManager.notify(NOTIFICATION_ID_BASE, notification)
    }

    /**
     * Show notification for incoming call.
     */
    private fun showCallNotification(senderPeerId: String) {
        val notificationManager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        createNotificationChannel(notificationManager, CALL_CHANNEL, "Calls", "Incoming call notifications")
        
        val intent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            putExtra("incoming_call", senderPeerId)
        }
        val pendingIntent = PendingIntent.getActivity(
            this, NOTIFICATION_ID_BASE + 1, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

        val notification = NotificationCompat.Builder(this, CALL_CHANNEL)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("Incoming Call")
            .setContentText("Incoming voice/video call")
            .setPriority(NotificationCompat.PRIORITY_MAX)
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setAutoCancel(true)
            .setContentIntent(pendingIntent)
            .build()

        notificationManager.notify(NOTIFICATION_ID_BASE + 1, notification)
    }

    /**
     * Show notification for group message.
     */
    private fun showGroupNotification(senderPeerId: String, data: Map<String, String>) {
        val notificationManager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        createNotificationChannel(notificationManager, MSG_CHANNEL, "Messages", "New message notifications")
        
        val intent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            putExtra("open_group", data["group_id"] ?: "")
        }
        val pendingIntent = PendingIntent.getActivity(
            this, NOTIFICATION_ID_BASE + 2, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

        val notification = NotificationCompat.Builder(this, MSG_CHANNEL)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("New Group Message")
            .setContentText("New message in group chat")
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setAutoCancel(true)
            .setContentIntent(pendingIntent)
            .build()

        notificationManager.notify(NOTIFICATION_ID_BASE + 2, notification)
    }

    /**
     * Create notification channel (required for Android 8+).
     */
    private fun createNotificationChannel(manager: NotificationManager, channelId: String, name: String, description: String) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                channelId, name, NotificationManager.IMPORTANCE_HIGH
            ).apply {
                this.description = description
                enableVibration(true)
            }
            manager.createNotificationChannel(channel)
        }
    }
}
