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
            const [success, stdout] = GLib.spawn_command_line_sync(
                'gdbus call --session --dest org.personal.CompanionAI --object-path /org/personal/CompanionAI/Companion --method org.personal.CompanionAI.Companion1.GetFormattedInsights "gnome"'
            );
            
            if (success && stdout) {
                const output = new TextDecoder().decode(stdout).trim();
                // Extract JSON from D-Bus response: ("JSON_STRING",)
                const match = output.match(/^\("(.+)"\,\)$/s);
                if (match) {
                    const jsonStr = match[1].replace(/\\"/g, '"').replace(/\\\\/g, '\\');
                    const data = JSON.parse(jsonStr);
                    
                    if (data.text) {
                        this._label.set_text(data.text);
                    }
                    if (data.tooltip && this._indicator) {
                        this._indicator.set_tooltip_text(data.tooltip);
                    }
                    return;
                }
            }
        } catch (error) {
            // Silent fallback
        }
        
        // Fallback
        this._label.set_text('📅');
        if (this._indicator) {
            this._indicator.set_tooltip_text('Jasper: Waiting for daemon...');
        }
    }
}