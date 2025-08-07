// GNOME Shell 48 compatible extension - Working ES6 modules with D-Bus
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import * as Extension from 'resource:///org/gnome/shell/extensions/extension.js';

export default class JasperExtension extends Extension.Extension {
    enable() {
        try {
            // Create indicator
            this._indicator = new PanelMenu.Button(0.0, 'Jasper AI Insights', false);
            
            // Create label with loading emoji initially
            this._label = new St.Label({
                text: '🔄',
                style_class: 'system-status-icon',
                y_align: Clutter.ActorAlign.CENTER,
            });
            
            this._indicator.add_child(this._label);
            Main.panel.addToStatusArea('jasper-ai-insights', this._indicator);
            
            console.log('[Jasper] Extension UI created successfully');
            
            // Set up refresh timer - check every 5 seconds
            this._timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 5000, () => {
                this._refreshInsights();
                return GLib.SOURCE_CONTINUE;
            });
            
            // Initial refresh after short delay
            GLib.timeout_add(GLib.PRIORITY_DEFAULT, 2000, () => {
                this._refreshInsights();
                return GLib.SOURCE_REMOVE;
            });
            
        } catch (error) {
            console.error('[Jasper] Error in enable():', error);
        }
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
        console.log('[Jasper] Extension disabled successfully');
    }
    
    _refreshInsights() {
        console.log('[Jasper] Refreshing insights via D-Bus');
        
        try {
            // Use gdbus call for reliable D-Bus communication
            const [success, stdout] = GLib.spawn_command_line_sync(
                'gdbus call --session --dest org.personal.CompanionAI --object-path /org/personal/CompanionAI/Companion --method org.personal.CompanionAI.Companion1.GetFormattedInsights "gnome"'
            );
            
            if (success && stdout) {
                const output = new TextDecoder().decode(stdout);
                console.log(`[Jasper] D-Bus response: ${output.substring(0, 100)}...`);
                
                // Parse D-Bus response format: ("JSON_STRING",)
                const match = output.match(/\("([^"]+)"/); 
                if (match) {
                    const jsonString = match[1];
                    // Unescape the JSON
                    const unescapedJson = jsonString.replace(/\\"/g, '"').replace(/\\\\/g, '\\');
                    
                    try {
                        const data = JSON.parse(unescapedJson);
                        console.log(`[Jasper] Parsed data: ${data.text}`);
                        
                        // Update emoji
                        if (data.text) {
                            this._label.set_text(data.text);
                        }
                        
                        // Update tooltip
                        if (data.tooltip && this._indicator) {
                            this._indicator.set_tooltip_text(data.tooltip);
                        }
                        
                        return;
                    } catch (parseError) {
                        console.warn(`[Jasper] JSON parse failed: ${parseError.message}`);
                    }
                }
            }
        } catch (error) {
            console.warn(`[Jasper] D-Bus call failed: ${error.message}`);
        }
        
        // Fallback to calendar emoji if daemon not available
        this._label.set_text('📅');
        if (this._indicator) {
            this._indicator.set_tooltip_text('Jasper: Waiting for daemon...');
        }
    }
}