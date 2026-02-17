import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Widgets

Item {
    id: root
    property var pluginApi: null

    // Panel geometry hints for SmartPanel
    readonly property var geometryPlaceholder: panelContainer
    readonly property bool allowAttach: true
    property real contentPreferredWidth:  320 * Style.uiScaleRatio
    property real contentPreferredHeight: col.implicitHeight + Style.marginXL * 2

    // Convenience aliases
    // NOTE: "state" is a built-in Item property (QML States) — using "insightState"
    // to avoid child items (NText, NButton) resolving to their own Item.state ("")
    readonly property var    jasper:       pluginApi?.mainInstance
    readonly property string emoji:        jasper?.currentEmoji   ?? ""
    readonly property string insight:      jasper?.currentInsight  ?? ""
    readonly property string insightState: jasper?.currentState   ?? "offline"
    readonly property int    iid:          jasper?.insightId      ?? 0
    readonly property bool   refreshing:   jasper?.isRefreshing   ?? false
    readonly property real   lastUpdated:  jasper?.lastUpdatedAt  ?? 0

    // Relative time string for footer
    readonly property string agoText: {
        if (root.lastUpdated <= 0) return "";
        var ago = Math.floor((Date.now() - root.lastUpdated) / 60000);
        if (ago < 1) return "Just now";
        if (ago === 1) return "1m ago";
        if (ago < 60) return ago + "m ago";
        var hours = Math.floor(ago / 60);
        if (hours === 1) return "1h ago";
        return hours + "h ago";
    }

    anchors.fill: parent

    Rectangle {
        id: panelContainer
        anchors.fill: parent
        color: "transparent"
    }

    ColumnLayout {
        id: col
        anchors.fill: parent
        anchors.margins: Style.marginXL
        spacing: Style.marginL

        // ── Hero: emoji + insight text ──
        RowLayout {
            Layout.fillWidth: true
            spacing: Style.marginM

            Text {
                text: root.emoji || "\u{1F4C5}"
                font.pixelSize: Style.fontSizeXXL * Style.uiScaleRatio * 1.6
                verticalAlignment: Text.AlignTop
                Layout.alignment: Qt.AlignTop
            }

            NText {
                Layout.fillWidth: true
                text: {
                    if (root.insightState === "active" && root.insight)
                        return root.insight;
                    if (root.insightState === "waiting")
                        return "Analyzing your context\u{2026}";
                    if (root.insightState === "error")
                        return "Error communicating with daemon.";
                    return "Jasper daemon is offline.";
                }
                pointSize: Style.fontSizeM
                font.weight: root.insightState === "active" ? Font.Medium : Font.Normal
                color: Color.mOnSurface
                wrapMode: Text.Wrap
                lineHeight: 1.3
            }
        }

        // ── Separator ──
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Color.mOutline
            opacity: 0.25
        }

        // ── Footer: timestamp + refresh ──
        RowLayout {
            Layout.fillWidth: true
            spacing: Style.marginM

            NText {
                text: {
                    if (root.refreshing) return "Refreshing\u{2026}";
                    if (root.insightState === "active" && root.agoText)
                        return root.agoText;
                    return root.insightState.charAt(0).toUpperCase() + root.insightState.slice(1);
                }
                pointSize: Style.fontSizeXS
                color: Color.mOnSurfaceVariant
                Layout.fillWidth: true
            }

            NButton {
                text: "Refresh"
                icon: "refresh"
                enabled: !root.refreshing && root.insightState !== "offline"
                onClicked: {
                    if (root.jasper) root.jasper.forceRefresh();
                }
            }
        }
    }
}
