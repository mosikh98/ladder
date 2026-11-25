package com.mosikh98.ladder

import android.app.Activity
import android.app.AlertDialog
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import org.json.JSONObject

class EditorActivity : Activity() {
    companion object {
        const val EXTRA_NAME = "name"
    }

    private lateinit var edit: EditText
    private lateinit var name: String

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()

    private fun pill(hex: String): GradientDrawable {
        val d = GradientDrawable()
        d.setColor(Color.parseColor(hex))
        d.cornerRadius = dp(14).toFloat()
        return d
    }

    private fun button(text: String, hex: String, onClick: () -> Unit): Button {
        val b = Button(this)
        b.text = text
        b.isAllCaps = false
        b.textSize = 14f
        b.setTextColor(Color.WHITE)
        b.background = pill(hex)
        b.setOnClickListener { onClick() }
        return b
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        name = intent.getStringExtra(EXTRA_NAME) ?: Env.CONFIG
        window.statusBarColor = Color.parseColor("#0B1F24")
        window.decorView.layoutDirection = View.LAYOUT_DIRECTION_RTL

        val root = LinearLayout(this)
        root.orientation = LinearLayout.VERTICAL
        root.setBackgroundColor(Color.parseColor("#0B1F24"))
        root.setPadding(dp(16), dp(16), dp(16), dp(16))

        val title = TextView(this)
        title.text = name
        title.textSize = 18f
        title.setTypeface(null, Typeface.BOLD)
        title.setTextColor(Color.WHITE)
        title.textDirection = View.TEXT_DIRECTION_LTR
        title.gravity = Gravity.START
        root.addView(
            title,
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        )

        edit = EditText(this)
        edit.typeface = Typeface.MONOSPACE
        edit.textSize = 12f
        edit.setTextColor(Color.parseColor("#CFE8E3"))
        edit.setBackgroundColor(Color.parseColor("#06141A"))
        edit.gravity = Gravity.TOP
        edit.textDirection = View.TEXT_DIRECTION_LTR
        edit.setPadding(dp(10), dp(10), dp(10), dp(10))
        edit.inputType = InputType.TYPE_CLASS_TEXT or
            InputType.TYPE_TEXT_FLAG_MULTI_LINE or
            InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        val f = Env.file(this, name)
        edit.setText(if (f.exists()) f.readText() else "")
        val ep = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f)
        ep.topMargin = dp(10)
        root.addView(edit, ep)

        val row = LinearLayout(this)
        row.orientation = LinearLayout.HORIZONTAL
        val buttons = ArrayList<Button>()
        buttons.add(button("ذخیره", "#1FA58A") { save() })
        if (name == Env.CONFIG) buttons.add(button("پیش‌فرض", "#1C4A52") { confirmReset() })
        buttons.add(button("بستن", "#1C4A52") { finish() })
        for (b in buttons) {
            val p = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            p.marginStart = dp(4)
            p.marginEnd = dp(4)
            row.addView(b, p)
        }
        val rp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        rp.topMargin = dp(10)
        root.addView(row, rp)

        setContentView(root)
    }

    private fun toast(msg: String) {
        Toast.makeText(this, msg, Toast.LENGTH_SHORT).show()
    }

    private fun save() {
        val text = edit.text.toString()
        if (name.endsWith(".json") && text.isNotBlank()) {
            try {
                JSONObject(text)
            } catch (e: Exception) {
                toast("JSON معتبر نیست: " + e.message)
                return
            }
        }
        Env.file(this, name).writeText(text)
        toast("ذخیره شد")
    }

    private fun confirmReset() {
        AlertDialog.Builder(this)
            .setTitle("بازگشت به تنظیمات پیش‌فرض؟")
            .setPositiveButton("بله") { _, _ ->
                Env.resetConfig(this)
                Env.prepare(this)
                edit.setText(Env.file(this, Env.CONFIG).readText())
                toast("برگشت")
            }
            .setNegativeButton("خیر", null)
            .show()
    }
}
