import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import * as Extension from 'resource:///org/gnome/shell/extensions/extension.js';

const DBUS_NAME = 'org.jasper.Daemon';
const DBUS_PATH = '/org/jasper/Daemon';
const DBUS_INTERFACE = 'org.jasper.Daemon1';

export default class JasperExtension extends Extension.Extension {
    enable() {
        this._indicator = new PanelMenu.Button(0.0, 'Jasper AI Insights', false);
        this._label = new St.Label({
            text: '🔄',
            style_class: 'system-status-icon',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._indicator.add_child(this._label);
        Main.panel.addToStatusArea('jasper-ai-insights', this._indicator);

        // Store insight text
        this._insightText = 'Jasper: Starting...';

        // Create popup menu
        this._item = new PopupMenu.PopupMenuItem('');
        this._item.label.clutter_text.set_line_wrap(true);
        this._item.label.style = 'max-width: 300px;';
        this._updateMenuText();
        this._indicator.menu.addMenuItem(this._item);

        // Add separator
        this._indicator.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        // Add refresh button
        let refreshItem = new PopupMenu.PopupMenuItem('🔄 Refresh Now');
        refreshItem.connect('activate', () => {
            this._refreshInsights();
            this._indicator.menu.close();
        });
        this._indicator.menu.addMenuItem(refreshItem);

        // State tracking
        this._registered = false;
        this._proxy = null;
        this._pid = 0;

        // Initialize D-Bus proxy asynchronously
        this._initProxy();

        // Recurring timer for insights and heartbeat
        this._timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 5000, () => {
            this._refreshInsights();
            this._sendHeartbeat();
            return GLib.SOURCE_CONTINUE;
        });
    }

    disable() {
        // Unregister from daemon before shutdown
        this._unregisterFromDaemon();

        if (this._timeoutId) {
            GLib.Source.remove(this._timeoutId);
            this._timeoutId = null;
        }

        // Clean up signal subscription
        if (this._signalSubscriptionId && this._proxy) {
            this._proxy.disconnect(this._signalSubscriptionId);
            this._signalSubscriptionId = null;
        }

        this._proxy = null;

        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }
        this._label = null;
        this._item = null;
        this._registered = false;
    }

    _updateMenuText() {
        if (this._item) {
            this._item.label.set_text(this._insightText);
        }
    }

    _initProxy() {
        // Create D-Bus proxy asynchronously
        Gio.DBusProxy.new_for_bus(
            Gio.BusType.SESSION,
            Gio.DBusProxyFlags.NONE,
            null, // GDBusInterfaceInfo
            DBUS_NAME,
            DBUS_PATH,
            DBUS_INTERFACE,
            null, // cancellable
            (source, result) => {
                try {
                    this._proxy = Gio.DBusProxy.new_for_bus_finish(result);

                    // Subscribe to InsightUpdated signal
                    this._signalSubscriptionId = this._proxy.connect('g-signal',
                        (proxy, senderName, signalName, parameters) => {
                            if (signalName === 'InsightUpdated') {
                                this._onInsightUpdated(parameters);
                            }
                        });

                    // Register with daemon now that proxy is ready
                    this._registerWithDaemon();
                } catch (e) {
                    this._label.set_text('📅');
                    this._insightText = 'Jasper: Daemon not available';
                    this._updateMenuText();
                }
            }
        );
    }

    _onInsightUpdated(parameters) {
        // Signal parameters: (insight_id, emoji, preview)
        const [insightId, emoji, preview] = parameters.deep_unpack();
        if (insightId > 0) {
            this._label.set_text(emoji || '🤖');
            this._insightText = `Jasper: ${preview}`;
            this._updateMenuText();
        }
    }

    _registerWithDaemon() {
        if (!this._proxy) {
            this._label.set_text('📅');
            this._insightText = 'Jasper: Daemon not available';
            this._updateMenuText();
            return;
        }

        this._proxy.call(
            'RegisterFrontend',
            new GLib.Variant('(si)', ['gnome-extension', this._pid]),
            Gio.DBusCallFlags.NONE,
            -1, // timeout
            null, // cancellable
            (proxy, result) => {
                try {
                    const reply = proxy.call_finish(result);
                    const [success] = reply.deep_unpack();

                    if (success) {
                        this._registered = true;
                        this._label.set_text('🔍');
                        this._insightText = 'Jasper: Registered, analyzing...';
                        this._updateMenuText();
                        this._refreshInsights();
                    } else {
                        this._onDaemonUnavailable();
                    }
                } catch (e) {
                    this._onDaemonUnavailable();
                }
            }
        );
    }

    _unregisterFromDaemon() {
        if (!this._registered || !this._proxy) return;

        // Use sync call during disable() since we're shutting down
        // This is acceptable as it's a one-time operation during extension disable
        try {
            this._proxy.call_sync(
                'UnregisterFrontend',
                new GLib.Variant('(s)', ['gnome-extension']),
                Gio.DBusCallFlags.NONE,
                1000, // 1 second timeout
                null
            );
        } catch (e) {
            // Ignore errors during unregistration
        }
        this._registered = false;
    }

    _sendHeartbeat() {
        if (!this._registered || !this._proxy) return;

        this._proxy.call(
            'Heartbeat',
            new GLib.Variant('(s)', ['gnome-extension']),
            Gio.DBusCallFlags.NONE,
            -1,
            null,
            (proxy, result) => {
                try {
                    const reply = proxy.call_finish(result);
                    const [success] = reply.deep_unpack();

                    if (!success) {
                        // Heartbeat failed, try to re-register
                        this._registered = false;
                        this._registerWithDaemon();
                    }
                } catch (e) {
                    // If heartbeat fails, we might need to re-register
                    this._registered = false;
                    this._registerWithDaemon();
                }
            }
        );
    }

    _refreshInsights() {
        if (!this._proxy) {
            this._initProxy();
            return;
        }

        if (!this._registered) {
            this._registerWithDaemon();
            return;
        }

        this._proxy.call(
            'GetLatestInsight',
            null, // no parameters
            Gio.DBusCallFlags.NONE,
            -1,
            null,
            (proxy, result) => {
                try {
                    const reply = proxy.call_finish(result);
                    const [id, emoji, insight, contextHash] = reply.deep_unpack();

                    if (id > 0) {
                        // We have a real insight
                        this._label.set_text(emoji || '🤖');
                        this._insightText = `Jasper: ${insight}`;
                        this._updateMenuText();
                    } else {
                        // No insights yet
                        this._label.set_text('🔍');
                        this._insightText = 'Jasper: Analyzing your context...';
                        this._updateMenuText();
                    }
                } catch (e) {
                    // D-Bus call failed
                    this._registered = false;
                    this._onDaemonUnavailable();
                }
            }
        );
    }

    _onDaemonUnavailable() {
        this._label.set_text('📅');
        this._insightText = 'Jasper: Daemon not available';
        this._updateMenuText();
    }
}
