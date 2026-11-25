package com.mosikh98.ladder

import android.content.Context
import java.io.File

object Env {
    const val CONFIG = "config.toml"
    const val BASE = "psiphon.base.json"
    const val PROXIES = "proxies.txt"
    const val SERVERS = "servers.json"
    const val CORE = "psiphon-tunnel-core"

    fun workDir(c: Context): File = File(c.filesDir, "work").also { it.mkdirs() }

    fun file(c: Context, name: String): File = File(workDir(c), name)

    fun ladderBin(c: Context): File = File(c.applicationInfo.nativeLibraryDir, "libladder.so")

    /** Imported core wins over one packed into the APK as libpsiphon.so. */
    fun coreBin(c: Context): File? {
        val imported = File(c.filesDir, CORE)
        if (imported.exists()) {
            imported.setExecutable(true, false)
            return imported
        }
        val packed = File(c.applicationInfo.nativeLibraryDir, "libpsiphon.so")
        return if (packed.exists()) packed else null
    }

    private fun asset(c: Context, name: String): String =
        c.assets.open(name).bufferedReader().use { it.readText() }

    fun defaultConfig(c: Context): String =
        asset(c, "config.example.toml").replace("examples/servers.example.json", SERVERS)

    fun resetConfig(c: Context) {
        file(c, CONFIG).writeText(defaultConfig(c))
    }

    /** Seed missing files and point `binary` at the current core path. */
    fun prepare(c: Context) {
        val cfg = file(c, CONFIG)
        if (!cfg.exists()) resetConfig(c)
        val servers = file(c, SERVERS)
        if (!servers.exists()) servers.writeText(asset(c, "servers.example.json"))
        val proxies = file(c, PROXIES)
        if (!proxies.exists()) proxies.writeText("")
        val core = coreBin(c)
        if (core != null) {
            val text = cfg.readText()
            val re = Regex("(?m)^binary\\s*=.*$")
            if (re.containsMatchIn(text)) {
                cfg.writeText(re.replace(text) { "binary = \"" + core.path + "\"" })
            }
        }
    }
}
