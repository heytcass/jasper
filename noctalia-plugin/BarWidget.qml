import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Modules.Bar.Extras
import qs.Services.UI
import qs.Widgets

Item {
    id: root

    property var pluginApi: null
    property ShellScreen screen

    // Standard bar widget properties (injected by BarWidgetLoader)
    property string widgetId: ""
    property string section: ""
    property int sectionWidgetIndex: -1
    property int sectionWidgetsCount: 0

    // ── Read live data from Main.qml singleton ──
    readonly property var jasper: pluginApi?.mainInstance
    readonly property string emoji:   jasper?.currentEmoji   ?? ""
    readonly property string insight: jasper?.currentInsight  ?? ""
    readonly property string state:   jasper?.currentState   ?? "offline"
    readonly property bool refreshing: jasper?.isRefreshing   ?? false

    // Display emoji — pick a fallback per state
    readonly property string displayText: {
        if (refreshing)             return "\u{1F504}";   // 🔄
        if (state === "active" && emoji !== "") return emoji;
        if (state === "waiting")    return "\u{1F50D}";   // 🔍
        if (state === "error")      return "\u{26A0}\u{FE0F}"; // ⚠️
        return "\u{1F4C5}";                                // 📅 offline
    }

    // Last-updated timestamp from Main.qml
    readonly property real lastUpdated: jasper?.lastUpdatedAt ?? 0

    // Tooltip — brief status label, not the insight (insight lives in the panel)
    readonly property string tooltipContent: {
        if (refreshing)
            return "Jasper \u{00B7} Refreshing\u{2026}";
        if (state === "active" && lastUpdated > 0) {
            var ago = Math.floor((Date.now() - lastUpdated) / 60000);
            if (ago < 1) return "Jasper \u{00B7} Just now";
            if (ago === 1) return "Jasper \u{00B7} 1m ago";
            return "Jasper \u{00B7} " + ago + "m ago";
        }
        if (state === "waiting")
            return "Jasper \u{00B7} Analyzing\u{2026}";
        if (state === "error")
            return "Jasper \u{00B7} Error";
        return "Jasper \u{00B7} Offline";
    }

    implicitWidth: pill.width
    implicitHeight: pill.height

    BarPill {
        id: pill
        screen: root.screen
        oppositeDirection: BarService.getPillDirection(root)

        icon: ""
        text: root.displayText
        forceOpen: true
        tooltipText: root.tooltipContent

        onClicked: {
            pluginApi.togglePanel(root.screen, root);
        }
        onRightClicked: {
            if (jasper) jasper.forceRefresh();
        }
    }
}
