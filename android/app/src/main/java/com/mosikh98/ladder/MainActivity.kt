package com.mosikh98.ladder

import android.app.Activity
import android.app.AlertDialog
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Spinner
import android.widget.TextView
import android.widget.Toast
import java.io.File

class MainActivity : Activity() {

    private class Mode(val title: String, val args: Array<String>)

    private val modes = listOf(
        Mode("سایفون : اجرا (نردبان)", arrayOf("psiphon", "run")),
        Mode("سایفون : اسکن آی‌پی تمیز", arrayOf("psiphon", "scan")),
        Mode("سایفون : تست پروکسی‌ها", arrayOf("psiphon", "proxies")),
        Mode("سایفون : نمایش برنامه", arrayOf("psiphon", "plan")),
        Mode("ویندسکرایب : نمایش برنامه", arrayOf("windscribe", "plan")),
        Mode("ویندسکرایب : رله OpenVPN-TCP", arrayOf("windscribe", "relay", "--proto", "openvpn-tcp")),
        Mode("ویندسکرایب : رله Stealth", arrayOf("windscribe", "relay", "--proto", "stealth")),
        Mode("ویندسکرایب : رله WStunnel", arrayOf("windscribe", "relay", "--proto", "wstunnel"))
    )

    private lateinit var status: TextView
    private lateinit var hint: TextView
    private lateinit var spinner: Spinner
    private lateinit var startBtn: Button
    private lateinit var logView: TextView
    private lateinit var logScroll: ScrollView
    private var pendingImport: String? = null

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()

    private fun pill(hex: String): GradientDrawable {
        val d = GradientDrawable()
        d.setColor(Color.parseColor(hex))
        d.cornerRadius = dp(14).toFloat()
        return d
    }

    private fun lp(h: Int, weight: Float, top: Int): LinearLayout.LayoutParams {
        val p = LinearLayout.LayoutParams(
            if (weight > 0f) 0 else ViewGroup.LayoutParams.MATCH_PARENT, h, weight
        )
        p.topMargin = dp(top)
        return p
    }

    private fun smallButton(text: String, onClick: () -> Unit): Button {
        val b = Button(this)
        b.text = text
        b.isAllCaps = false
        b.textSize = 13f
        b.setTextColor(Color.WHITE)
        b.background = pill("#1C4A52")
        b.setOnClickListener { onClick() }
        return b
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        pendingImport = savedInstanceState?.getString("pi")
        window.statusBarColor = Color.parseColor("#0B1F24")
        window.decorView.layoutDirection = View.LAYOUT_DIRECTION_RTL

        val root = LinearLayout(this)
        root.orientation = LinearLayout.VERTICAL
        root.setBackgroundColor(Color.parseColor("#0B1F24"))
        root.setPadding(dp(16), dp(16), dp(16), dp(16))

        val title = TextView(this)
        title.text = "نردبان"
        title.textSize = 26f
        title.setTypeface(null, Typeface.BOLD)
        title.setTextColor(Color.WHITE)
        root.addView(title, lp(ViewGroup.LayoutParams.WRAP_CONTENT, 0f, 0))

        status = TextView(this)
        status.textSize = 14f
        status.setTextColor(Color.parseColor("#9FB8BD"))
        root.addView(status, lp(ViewGroup.LayoutParams.WRAP_CONTENT, 0f, 4))

        hint = TextView(this)
        hint.textSize = 13f
        hint.setTextColor(Color.parseColor("#F2C14E"))
        root.addView(hint, lp(ViewGroup.LayoutParams.WRAP_CONTENT, 0f, 8))

        spinner = Spinner(this)
        val adapter = ArrayAdapter(this, android.R.layout.simple_spinner_item, modes.map { it.title })
        adapter.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item)
        spinner.adapter = adapter
        root.addView(spinner, lp(ViewGroup.LayoutParams.WRAP_CONTENT, 0f, 8))

        startBtn = Button(this)
        startBtn.isAllCaps = false
        startBtn.textSize = 17f
        startBtn.setTextColor(Color.WHITE)
        startBtn.setOnClickListener { onStartStop() }
        root.addView(startBtn, lp(dp(54), 0f, 8))

        logView = TextView(this)
        logView.typeface = Typeface.MONOSPACE
        logView.textSize = 12f
        logView.setTextColor(Color.parseColor("#CFE8E3"))
        logView.setTextIsSelectable(true)
        logView.textDirection = View.TEXT_DIRECTION_LTR
        logView.setPadding(dp(10), dp(10), dp(10), dp(10))
        logScroll = ScrollView(this)
        logScroll.setBackgroundColor(Color.parseColor("#06141A"))
        logScroll.addView(logView)
        root.addView(logScroll, lp(0, 1f, 12).also { it.width = ViewGroup.LayoutParams.MATCH_PARENT })

        val row = LinearLayout(this)
        row.orientation = LinearLayout.HORIZONTAL
        val b1 = smallButton("ویرایش فایل‌ها") { pickEdit() }
        val b2 = smallButton("وارد کردن فایل") { pickImport() }
        val b3 = smallButton("کپی لاگ") { copyLog() }
        for (b in listOf(b1, b2, b3)) {
            val p = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            p.marginStart = dp(4)
            p.marginEnd = dp(4)
            row.addView(b, p)
        }
        root.addView(row, lp(ViewGroup.LayoutParams.WRAP_CONTENT, 0f, 10))

        setContentView(root)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putString("pi", pendingImport)
    }

    override fun onResume() {
        super.onResume()
        LadderLog.listener = { refresh() }
        refresh()
    }

    override fun onPause() {
        LadderLog.listener = null
        super.onPause()
    }

    private fun onStartStop() {
        if (LadderLog.running) {
            startService(Intent(this, LadderService::class.java).setAction(LadderService.ACTION_STOP))
            return
        }
        val m = modes[spinner.selectedItemPosition]
        val i = Intent(this, LadderService::class.java)
            .setAction(LadderService.ACTION_START)
            .putExtra(LadderService.EXTRA_ARGS, m.args)
            .putExtra(LadderService.EXTRA_LABEL, m.title)
        startForegroundService(i)
    }

    private fun refresh() {
        val running = LadderLog.running
        val log = LadderLog.snapshot()
        startBtn.text = if (running) "توقف" else "شروع"
        startBtn.background = pill(if (running) "#C0392B" else "#1FA58A")
        spinner.isEnabled = !running

        val up = log.lines().lastOrNull { it.startsWith("up. local socks") }
        status.text = when {
            running && up != null -> "متصل ✓   SOCKS " + up.removePrefix("up. local socks").trim()
            running -> "در حال اجرا : " + LadderLog.label
            else -> "متوقف"
        }

        val parts = ArrayList<String>()
        if (Env.coreBin(this) == null) {
            parts.add("• هستهٔ سایفون (psiphon-tunnel-core) پیدا نشد. از «وارد کردن فایل» بدهید.")
        }
        if (!Env.file(this, Env.BASE).exists()) {
            parts.add("• فایل psiphon.base.json هنوز نیست. از «ویرایش فایل‌ها» پیست کنید.")
        }
        hint.text = parts.joinToString("\n")
        hint.visibility = if (parts.isEmpty()) View.GONE else View.VISIBLE

        logView.text = log
        logScroll.post { logScroll.scrollTo(0, logView.height) }
    }

    private fun toast(msg: String) {
        Toast.makeText(this, msg, Toast.LENGTH_LONG).show()
    }

    private fun copyLog() {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("ladder log", LadderLog.snapshot()))
        toast("لاگ کپی شد")
    }

    private fun pickEdit() {
        val names = arrayOf(Env.CONFIG, Env.BASE, Env.PROXIES, Env.SERVERS)
        AlertDialog.Builder(this)
            .setTitle("ویرایش فایل")
            .setItems(names) { _, i ->
                Env.prepare(this)
                startActivity(
                    Intent(this, EditorActivity::class.java).putExtra(EditorActivity.EXTRA_NAME, names[i])
                )
            }
            .show()
    }

    private fun pickImport() {
        val labels = arrayOf(
            "هستهٔ سایفون (باینری psiphon-tunnel-core)",
            "psiphon.base.json",
            "proxies.txt",
            "servers.json (ویندسکرایب)"
        )
        val targets = arrayOf(Env.CORE, Env.BASE, Env.PROXIES, Env.SERVERS)
        AlertDialog.Builder(this)
            .setTitle("وارد کردن از فایل")
            .setItems(labels) { _, i ->
                pendingImport = targets[i]
                val pick = Intent(Intent.ACTION_OPEN_DOCUMENT)
                    .addCategory(Intent.CATEGORY_OPENABLE)
                    .setType("*/*")
                startActivityForResult(pick, 42)
            }
            .show()
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != 42 || resultCode != RESULT_OK) return
        val uri = data?.data ?: return
        val target = pendingImport ?: return
        try {
            val dest = if (target == Env.CORE) File(filesDir, Env.CORE) else Env.file(this, target)
            val input = contentResolver.openInputStream(uri)
            if (input == null) {
                toast("فایل خوانده نشد")
                return
            }
            input.use { src -> dest.outputStream().use { out -> src.copyTo(out) } }
            if (target == Env.CORE) {
                dest.setExecutable(true, false)
                val magic = dest.inputStream().use { s ->
                    val b = ByteArray(4)
                    val n = s.read(b)
                    if (n == 4) b else ByteArray(4)
                }
                val elf = magic[0] == 0x7F.toByte() && magic[1] == 'E'.code.toByte() &&
                    magic[2] == 'L'.code.toByte() && magic[3] == 'F'.code.toByte()
                if (!elf) toast("هشدار: این فایل ELF نیست (باینری اندروید/لینوکس arm64 لازم است)")
                else toast("وارد شد: " + target + " (" + dest.length() + " بایت)")
            } else {
                toast("وارد شد: " + target)
            }
        } catch (e: Exception) {
            toast("خطا: " + e.message)
        }
        refresh()
    }
}
