import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import * as Extension from 'resource:///org/gnome/shell/extensions/extension.js';

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
        
        // Immediate first call
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 1000, () => {
            this._refreshInsights();
            return GLib.SOURCE_REMOVE;
        });
        
        // Recurring timer
        this._timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 5000, () => {
            this._refreshInsights();
            return GLib.SOURCE_CONTINUE;
        });
    }

    disable() {
        if (this._timeoutId) {
            GLib.Source.remove(this._timeoutId);
            this._timeoutId = null;
        }
        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }
        this._label = null;
    }
    
    _refreshInsights() {
        try {
            const [success, stdout, stderr] = GLib.spawn_command_line_sync(
                'gdbus call --session --dest org.personal.CompanionAI --object-path /org/personal/CompanionAI/Companion --method org.personal.CompanionAI.Companion1.GetFormattedInsights "gnome"'
            );
            
            if (success && stdout && stdout.length > 0) {
                const output = new TextDecoder().decode(stdout).trim();
                
                // Try multiple regex patterns to match D-Bus response formats
                let jsonStr = null;
                
                // Pattern 1: ("JSON_STRING",)
                let match = output.match(/^\("(.*)"\s*,?\s*\)$/s);
                if (match) {
                    jsonStr = match[1];
                } else {
                    // Pattern 2: (JSON_STRING,)
                    match = output.match(/^\((.*),?\s*\)$/s);
                    if (match) {
                        jsonStr = match[1];
                    } else {
                        // Pattern 3: Just the JSON string
                        if (output.startsWith('{') && output.endsWith('}')) {
                            jsonStr = output;
                        }
                    }
                }
                
                if (jsonStr) {
                    // Clean up escaped quotes and backslashes
                    jsonStr = jsonStr.replace(/\\"/g, '"').replace(/\\\\/g, '\\');
                    
                    try {
                        const data = JSON.parse(jsonStr);
                        
                        if (data.text) {
                            this._label.set_text(data.text);
                        }
                        if (data.tooltip && this._indicator) {
                            this._indicator.set_tooltip_text(data.tooltip);
                        }
                        return;
                    } catch (parseError) {
                        console.warn('[Jasper] JSON parse failed:', parseError.message);
                    }
                }
            }
        } catch (error) {
            console.warn('[Jasper] D-Bus call failed:', error.message);
        }
        
        // Fallback
        this._label.set_text('📅');
        if (this._indicator) {
            this._indicator.set_tooltip_text('Jasper: Waiting for daemon...');
        }
    }
}