package expo.modules.takususerver

import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import expo.modules.kotlin.records.Record
import expo.modules.kotlin.records.Field
import expo.modules.kotlin.exception.CodedException
import uniffi.takusu_android.TakusuServer
import uniffi.takusu_android.ServerStatus
import uniffi.takusu_android.getLogs
import uniffi.takusu_android.clearLogs

class StartOptions : Record {
  @Field val port: Int = 3838
  @Field val workersUrl: String = ""
  @Field val rootToken: String = ""
}

class TakusuServerModule : Module() {
  private var server: TakusuServer? = null

  override fun definition() = ModuleDefinition {
    Name("TakusuServer")

    Function("start") { options: StartOptions ->
      try {
        if (server != null) {
          throw CodedException("ERR_ALREADY_RUNNING", "Server already running", null)
        }
        val instance = TakusuServer()
        instance.start(options.port.toUShort(), options.workersUrl, options.rootToken)
        server = instance
        true
      } catch (e: CodedException) {
        throw e
      } catch (e: Exception) {
        throw CodedException("ERR_START_FAILED", "Failed to start server: ${e.message}", e)
      }
    }

    Function("stop") {
      try {
        val s = server ?: throw CodedException("ERR_NOT_RUNNING", "Server not running", null)
        s.stop()
        server = null
        true
      } catch (e: CodedException) {
        throw e
      } catch (e: Exception) {
        throw CodedException("ERR_STOP_FAILED", "Failed to stop server: ${e.message}", e)
      }
    }

    Function("status") {
      try {
        val s = server
        if (s != null) {
          when (val result = s.status()) {
            is ServerStatus.Running -> mapOf("running" to true, "port" to result.port.toInt())
            is ServerStatus.Stopped -> mapOf("running" to false, "port" to 0)
          }
        } else {
          mapOf("running" to false, "port" to 0)
        }
      } catch (e: Exception) {
        mapOf("running" to false, "port" to 0)
      }
    }

    Function("getLogs") {
      try {
        getLogs()
      } catch (e: Exception) {
        emptyList<String>()
      }
    }

    Function("clearLogs") {
      try {
        clearLogs()
        true
      } catch (e: Exception) {
        false
      }
    }
  }

  companion object {
    init {
      try {
        System.loadLibrary("takusu_android")
      } catch (e: UnsatisfiedLinkError) {
        // Library not loaded yet — will be available after native build
      }
    }
  }
}
