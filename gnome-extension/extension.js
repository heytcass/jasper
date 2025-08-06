import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import St from 'gi://St';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';

export default class JasperExtension extends Extension {
    constructor(metadata) {
        super(metadata);
        this._indicator = null;
        this._label = null;
    }

    enable() {
        // Create the panel button
        this._indicator = new PanelMenu.Button(0.0, this.metadata.name, false);
        
        // Add emoji label for AI-chosen emoji
        this._label = new St.Label({
            text: '🔄',  // Loading emoji initially
            style_class: 'system-status-icon',
            y_align: Clutter.ActorAlign.CENTER,  // Fix vertical alignment
            x_align: Clutter.ActorAlign.CENTER,  // Center horizontally too
        });
        this._indicator.add_child(this._label);
        
        // Add to panel
        Main.panel.addToStatusArea(this.uuid, this._indicator);
        
        // Start refreshing insights after extension is fully loaded
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 2000, () => {
            this._refreshInsights();
            return GLib.SOURCE_REMOVE;
        });
    }

    disable() {
        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }
        this._label = null;
    }

    _refreshInsights() {
        try {
            // Show loading emoji
            if (this._label) {
                this._label.set_text('🧪');
            }
            
            // Call D-Bus service
            const launcher = new Gio.SubprocessLauncher({
                flags: Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE,
            });
            
            const proc = launcher.spawnv([
                '/run/current-system/sw/bin/dbus-send', '--session', '--dest=org.personal.CompanionAI',
                '--print-reply', '--type=method_call',
                '/org/personal/CompanionAI/Companion',
                'org.personal.CompanionAI.Companion1.GetFormattedInsights',
                'string:gnome'
            ]);
            
            // Use async communication
            proc.communicate_utf8_async(null, null, (proc, res) => {
                try {
                    const [, stdout, stderr] = proc.communicate_utf8_finish(res);
                    
                    if (proc.get_successful()) {
                        this._parseInsightsResponse(stdout);
                    } else {
                        this._setFallbackState();
                    }
                } catch (e) {
                    this._setFallbackState();
                }
            });
        } catch (error) {
            this._setFallbackState();
        }
    }

    _parseInsightsResponse(dbusOutput) {
        try {
            // Extract JSON from D-Bus output
            const lines = dbusOutput.split('\n');
            let jsonStr = '';
            
            for (const line of lines) {
                if (line.includes('string "')) {
                    const startIdx = line.indexOf('string "') + 8;
                    jsonStr = line.substring(startIdx);
                    if (jsonStr.endsWith('"')) {
                        jsonStr = jsonStr.slice(0, -1);
                    }
                    break;
                }
            }
            
            if (jsonStr) {
                const data = JSON.parse(jsonStr);
                this._updateUI(data);
            } else {
                this._setFallbackState();
            }
        } catch (error) {
            this._setFallbackState();
        }
    }

    _updateUI(insightData) {
        // Update panel emoji with AI-chosen emoji
        if (this._label && insightData.text) {
            this._label.set_text(insightData.text);
        }
        
        // Update tooltip using the correct GNOME Shell method
        if (this._indicator && insightData.tooltip) {
            this._indicator.actor.set_tooltip_text(insightData.tooltip);
        }
    }

    _setFallbackState() {
        if (this._label) {
            this._label.set_text('📅');
        }
        if (this._indicator) {
            this._indicator.actor.set_tooltip_text('Jasper: Daemon not available');
        }
    }
}