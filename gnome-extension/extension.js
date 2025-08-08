import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
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
        
        // Store insight text
        this._insightText = 'Jasper: Loading...';
        
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
        this._item = null;
    }
    
    _updateMenuText() {
        if (this._item) {
            this._item.label.set_text(this._insightText);
        }
    }
    
    _refreshInsights() {
        try {
            const [success, stdout, stderr] = GLib.spawn_command_line_sync(
                'gdbus call --session --dest org.personal.CompanionAI --object-path /org/personal/CompanionAI/Companion --method org.personal.CompanionAI.Companion1.GetFormattedInsights "gnome"'
            );
            
            if (success && stdout && stdout.length > 0) {
                const output = new TextDecoder().decode(stdout).trim();
                
                // D-Bus returns: ("JSON_STRING",)
                // Extract the JSON string from the D-Bus tuple format
                const match = output.match(/^\("(.*)"\s*,?\s*\)$/s);
                if (match) {
                    let jsonStr = match[1];
                    
                    // D-Bus escapes quotes and newlines - convert back to proper JSON
                    jsonStr = jsonStr.replace(/\\"/g, '"')  // Unescape quotes
                                    .replace(/\\n/g, '\n')  // Unescape newlines
                                    .replace(/\\\\/g, '\\'); // Unescape backslashes
                    
                    try {
                        const data = JSON.parse(jsonStr);
                        
                        if (data.text) {
                            this._label.set_text(data.text);
                        }
                        if (data.tooltip) {
                            this._insightText = data.tooltip;
                            this._updateMenuText();
                        }
                        return;
                    } catch (parseError) {
                        // Fall through to fallback
                    }
                }
            }
        } catch (error) {
            // Fall through to fallback
        }
        
        // Fallback
        this._label.set_text('📅');
        this._insightText = 'Jasper: Waiting for daemon...';
        this._updateMenuText();
    }
}