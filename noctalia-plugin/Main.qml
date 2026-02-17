import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root
    property var pluginApi: null

    // ── Shared state — other entry points read via pluginApi.mainInstance ──
    property int    insightId: 0
    property string currentEmoji: ""
    property string currentInsight: ""
    property string currentState: "offline"   // active | waiting | error | offline
    property bool   isRefreshing: false
    property real   lastUpdatedAt: 0          // epoch ms of last successful insight

    // Poll interval from plugin settings (default 30 s)
    readonly property int pollInterval: {
        var ms = pluginApi?.pluginSettings?.pollIntervalMs;
        return (ms && ms > 0) ? ms : 30000;
    }

    // ── IPC handlers (qs -c noctalia-shell ipc call plugin:jasper-insights <cmd>) ──
    IpcHandler {
        target: "plugin:jasper-insights"

        function refresh() {
            root.forceRefresh();
        }

        function toggle() {
            pluginApi.withCurrentScreen(function(screen) {
                pluginApi.togglePanel(screen);
            });
        }
    }

    // ── Polling timer ──
    Timer {
        id: pollTimer
        interval: root.pollInterval
        repeat: true
        running: true
        triggeredOnStart: true
        onTriggered: root.poll()
    }

    // ── Process: periodic poll ──
    Process {
        id: pollProc
        command: ["jasper-companion-daemon", "noctalia"]
        running: false
        stdout: StdioCollector {
            onStreamFinished: root.parseOutput(this.text)
        }
        onExited: function(exitCode, exitStatus) {
            if (exitCode !== 0) {
                root.currentState = "offline";
            }
        }
    }

    // ── Process: force-refresh (separate so they don't collide) ──
    Process {
        id: refreshProc
        command: ["jasper-companion-daemon", "noctalia-refresh"]
        running: false
        stdout: StdioCollector {
            onStreamFinished: {
                root.parseOutput(this.text);
                root.isRefreshing = false;
            }
        }
        onExited: function(exitCode, exitStatus) {
            if (exitCode !== 0) {
                root.currentState = "error";
            }
            root.isRefreshing = false;
        }
    }

    // ── Actions ──
    function poll() {
        if (!pollProc.running && !refreshProc.running) {
            pollProc.running = true;
        }
    }

    function forceRefresh() {
        if (!refreshProc.running) {
            isRefreshing = true;
            refreshProc.running = true;
            pollTimer.restart();   // avoid double-hit right after
        }
    }

    function parseOutput(text) {
        var trimmed = (text || "").trim();
        if (!trimmed) {
            currentState = "offline";
            return;
        }
        try {
            var data = JSON.parse(trimmed);
            insightId      = data.id      || 0;
            currentEmoji   = data.emoji   || "";
            currentInsight = data.insight  || "";
            currentState   = data.state   || "offline";
            if (currentState === "active") lastUpdatedAt = Date.now();
        } catch (e) {
            currentState = "error";
            currentEmoji = "";
            currentInsight = "";
        }
    }
}
