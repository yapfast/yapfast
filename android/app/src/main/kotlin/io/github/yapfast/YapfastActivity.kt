package io.github.yapfast

import android.Manifest
import android.app.Activity
import android.app.NativeActivity
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import android.view.View
import android.view.WindowInsetsController
import android.view.WindowManager
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger

class YapfastActivity : NativeActivity() {

    private sealed class PendingRequest {
        data class Permission(val requestId: Long) : PendingRequest()
        data class PickFiles(val requestId: Long) : PendingRequest()
        data class SaveFile(val requestId: Long, val fileName: String) : PendingRequest()
    }

    private val nextCode = AtomicInteger(100)
    private val pendingRequests = ConcurrentHashMap<Int, PendingRequest>()
    private val pendingSaveUris = ConcurrentHashMap<String, Uri>()
    private val ioExecutor = Executors.newSingleThreadExecutor()

    override fun onCreate(savedInstanceState: Bundle?) {
        configureSystemBars()
        super.onCreate(savedInstanceState)
        ensureNotificationChannels()
        maybeRequestNotificationPermission()
        handleIntent(intent)
    }

    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        if (intent != null) {
            setIntent(intent)
            handleIntent(intent)
        }
    }

    override fun onDestroy() {
        ioExecutor.shutdownNow()
        super.onDestroy()
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        val pending = pendingRequests.remove(requestCode)
        if (pending is PendingRequest.Permission) {
            val granted = grantResults.isNotEmpty() && grantResults[0] == PackageManager.PERMISSION_GRANTED
            nativeOnPermissionResult(pending.requestId, granted)
            return
        }
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        when (val pending = pendingRequests.remove(requestCode)) {
            is PendingRequest.PickFiles -> {
                if (resultCode != Activity.RESULT_OK || data == null) {
                    nativeOnFilesPicked(pending.requestId, null)
                    return
                }
                val uris = collectPickedUris(data)
                if (uris.isEmpty()) {
                    nativeOnFilesPicked(pending.requestId, null)
                    return
                }
                ioExecutor.execute {
                    val paths = copyPickedUris(pending.requestId, uris)
                    nativeOnFilesPicked(pending.requestId, paths.takeIf { it.isNotEmpty() }?.toTypedArray())
                }
            }
            is PendingRequest.SaveFile -> {
                val uri = if (resultCode == Activity.RESULT_OK) data?.data else null
                if (uri == null) {
                    nativeOnSaveTarget(pending.requestId, null)
                    return
                }
                val dir = File(cacheDir, "saves/${pending.requestId}").apply { mkdirs() }
                val safeName = File(pending.fileName).name.ifBlank { "download" }
                val stagedFile = File(dir, safeName)
                pendingSaveUris[stagedFile.absolutePath] = uri
                nativeOnSaveTarget(pending.requestId, stagedFile.absolutePath)
            }
            else -> super.onActivityResult(requestCode, resultCode, data)
        }
    }

    @Suppress("unused")
    fun hasRuntimePermission(permission: String): Boolean {
        return checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED
    }

    @Suppress("unused")
    fun requestRuntimePermission(requestId: Long, permission: String) {
        runOnUiThread {
            if (checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED) {
                nativeOnPermissionResult(requestId, true)
            } else {
                val code = allocateRequestCode()
                pendingRequests[code] = PendingRequest.Permission(requestId)
                try {
                    requestPermissions(arrayOf(permission), code)
                } catch (_: Throwable) {
                    pendingRequests.remove(code)
                    nativeOnPermissionResult(requestId, false)
                }
            }
        }
    }

    @Suppress("unused")
    fun openUrl(url: String) {
        runOnUiThread {
            try {
                val browser = Intent(Intent.ACTION_VIEW, Uri.parse(url)).apply {
                    addCategory(Intent.CATEGORY_BROWSABLE)
                }
                startActivity(browser)
            } catch (_: Throwable) {
            }
        }
    }

    @Suppress("unused")
    fun showMessageNotification(room: String, title: String, body: String, isCall: Boolean) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        val manager = getSystemService(NotificationManager::class.java) ?: return
        val channelId = if (isCall) CALL_CHANNEL_ID else MESSAGE_CHANNEL_ID
        val openIntent = Intent(this, YapfastActivity::class.java).apply {
            putExtra(EXTRA_ROOM, room)
            addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        }
        val requestCode = (room.hashCode() and 0x7FFF) + 1000
        val contentPending = PendingIntent.getActivity(
            this,
            requestCode,
            openIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

        val notification = Notification.Builder(this, channelId)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(title.ifBlank { getString(R.string.app_name) })
            .setContentText(body)
            .setStyle(Notification.BigTextStyle().bigText(body))
            .setAutoCancel(true)
            .setCategory(if (isCall) Notification.CATEGORY_CALL else Notification.CATEGORY_MESSAGE)
            .setContentIntent(contentPending)
            .build()

        manager.notify(requestCode, notification)
    }

    @Suppress("unused")
    fun setCallActive(active: Boolean, title: String) {
        runOnUiThread {
            if (active) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R &&
                    checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED
                ) {
                    return@runOnUiThread
                }
                val serviceIntent = Intent(this, YapfastCallService::class.java).apply {
                    putExtra(YapfastCallService.EXTRA_TITLE, title)
                }
                try {
                    startForegroundService(serviceIntent)
                } catch (_: Throwable) {
                }
            } else {
                stopService(Intent(this, YapfastCallService::class.java))
            }
        }
    }

    @Suppress("unused")
    fun pickFiles(requestId: Long, allowMultiple: Boolean, mimeType: String) {
        runOnUiThread {
            val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = mimeType.ifBlank { "*/*" }
                putExtra(Intent.EXTRA_ALLOW_MULTIPLE, allowMultiple)
            }
            val code = allocateRequestCode()
            pendingRequests[code] = PendingRequest.PickFiles(requestId)
            try {
                startActivityForResult(intent, code)
            } catch (_: Throwable) {
                pendingRequests.remove(code)
                nativeOnFilesPicked(requestId, null)
            }
        }
    }

    @Suppress("unused")
    fun prepareSaveFile(requestId: Long, title: String, fileName: String) {
        runOnUiThread {
            val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = "*/*"
                putExtra(Intent.EXTRA_TITLE, fileName.ifBlank { "download" })
            }
            val code = allocateRequestCode()
            pendingRequests[code] = PendingRequest.SaveFile(requestId, fileName)
            try {
                startActivityForResult(Intent.createChooser(intent, title), code)
            } catch (_: Throwable) {
                pendingRequests.remove(code)
                nativeOnSaveTarget(requestId, null)
            }
        }
    }

    @Suppress("unused")
    fun finishSaveFile(path: String, ok: Boolean) {
        ioExecutor.execute {
            val uri = pendingSaveUris.remove(path)
            val stagedFile = File(path)
            try {
                if (ok && uri != null && stagedFile.isFile) {
                    contentResolver.openOutputStream(uri, "wt")?.use { out ->
                        stagedFile.inputStream().use { input ->
                            input.copyTo(out)
                        }
                    }
                }
            } catch (_: Throwable) {
            } finally {
                stagedFile.delete()
                stagedFile.parentFile?.let { parent ->
                    if (parent.list()?.isEmpty() == true) {
                        parent.delete()
                    }
                }
            }
        }
    }

    private fun allocateRequestCode(): Int {
        while (true) {
            val candidate = (nextCode.getAndIncrement() and 0x7FFF).coerceAtLeast(100)
            if (!pendingRequests.containsKey(candidate)) {
                return candidate
            }
        }
    }

    private fun collectPickedUris(data: Intent): List<Uri> {
        val result = mutableListOf<Uri>()
        val clip = data.clipData
        if (clip != null) {
            for (index in 0 until clip.itemCount) {
                clip.getItemAt(index)?.uri?.let(result::add)
            }
        } else {
            data.data?.let(result::add)
        }
        return result
    }

    private fun copyPickedUris(requestId: Long, uris: List<Uri>): List<String> {
        val folder = File(cacheDir, "picked/$requestId").apply { mkdirs() }
        val out = mutableListOf<String>()
        uris.forEachIndexed { index, uri ->
            val rawName = queryDisplayName(uri) ?: "file_$index"
            val safeName = rawName.replace('/', '_').replace('\\', '_').ifBlank { "file_$index" }
            val dest = File(folder, if (uris.size > 1) "${index}_$safeName" else safeName)
            try {
                contentResolver.openInputStream(uri)?.use { input ->
                    dest.outputStream().use { output ->
                        input.copyTo(output)
                    }
                }
                if (dest.isFile) {
                    out.add(dest.absolutePath)
                }
            } catch (_: Throwable) {
            }
        }
        return out
    }

    private fun queryDisplayName(uri: Uri): String? {
        return try {
            contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (idx >= 0) cursor.getString(idx) else null
                } else {
                    null
                }
            }
        } catch (_: Throwable) {
            null
        }
    }

    private fun handleIntent(intent: Intent?) {
        if (intent == null) return
        val room = intent.getStringExtra(EXTRA_ROOM)
        if (!room.isNullOrBlank()) {
            nativeOnOpenRoom(room)
        }
        val uri = intent.data?.toString()
        if (intent.action == Intent.ACTION_VIEW && !uri.isNullOrBlank()) {
            nativeOnDeepLink(uri)
        }
    }

    private fun ensureNotificationChannels() {
        val manager = getSystemService(NotificationManager::class.java) ?: return
        if (manager.getNotificationChannel(MESSAGE_CHANNEL_ID) == null) {
            manager.createNotificationChannel(
                NotificationChannel(
                    MESSAGE_CHANNEL_ID,
                    getString(R.string.message_notification_channel),
                    NotificationManager.IMPORTANCE_DEFAULT,
                ),
            )
        }
        if (manager.getNotificationChannel(CALL_CHANNEL_ID) == null) {
            manager.createNotificationChannel(
                NotificationChannel(
                    CALL_CHANNEL_ID,
                    getString(R.string.call_notification_channel),
                    NotificationManager.IMPORTANCE_HIGH,
                ),
            )
        }
    }

    private fun maybeRequestNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        val prefs = getSharedPreferences("yapfast_prefs", MODE_PRIVATE)
        if (prefs.getBoolean("asked_notifications", false)) return
        prefs.edit().putBoolean("asked_notifications", true).apply()
        val code = allocateRequestCode()
        pendingRequests[code] = PendingRequest.Permission(-1L)
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), code)
    }

    @Suppress("DEPRECATION")
    private fun configureSystemBars() {
        window.addFlags(WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS)
        window.statusBarColor = Color.TRANSPARENT
        window.navigationBarColor = Color.TRANSPARENT
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            window.isNavigationBarContrastEnforced = false
            window.isStatusBarContrastEnforced = false
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            window.setDecorFitsSystemWindows(false)
            window.insetsController?.setSystemBarsAppearance(
                0,
                WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
                    WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS,
            )
        } else {
            window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
                View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        }
    }

    companion object {
        private const val MESSAGE_CHANNEL_ID = "yapfast_messages"
        private const val CALL_CHANNEL_ID = "yapfast_calls"
        private const val EXTRA_ROOM = "room"

        init {
            System.loadLibrary("yapfast")
        }

        @JvmStatic
        external fun nativeOnFilesPicked(requestId: Long, paths: Array<String>?)

        @JvmStatic
        external fun nativeOnSaveTarget(requestId: Long, path: String?)

        @JvmStatic
        external fun nativeOnOpenRoom(room: String)

        @JvmStatic
        external fun nativeOnDeepLink(link: String)

        @JvmStatic
        external fun nativeOnPermissionResult(requestId: Long, granted: Boolean)

        @JvmStatic
        external fun nativeOnDisconnectCall()
    }
}
