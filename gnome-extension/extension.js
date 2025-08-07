// Jasper GNOME Shell Extension - ES6 Modules version with D-Bus
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import * as Extension from 'resource:///org/gnome/shell/extensions/extension.js';

export default class JasperExtension extends Extension.Extension {
    constructor(...args) {
        super(...args);
        this._indicator = null;
        this._label = null;
        this._timeoutId = null;
    }

    enable() {
        this._logMessage("Jasper extension enable() called - creating UI elements");
        
        // Create indicator
        this._indicator = new PanelMenu.Button(0.0, 'Jasper AI Insights', false);
        
        // Create label with initial loading emoji
        this._label = new St.Label({
            text: '🔄',
            style_class: 'system-status-icon',
            y_align: Clutter.ActorAlign.CENTER,
            x_align: Clutter.ActorAlign.CENTER,
        });
        
        this._indicator.add_child(this._label);
        Main.panel.addToStatusArea('jasper-ai-insights', this._indicator);
        
        this._logMessage("Jasper extension UI created and added to panel");
        
        // Set up refresh timer - check every 5 seconds
        this._timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 5000, () => {
            this._refreshInsights();
            return GLib.SOURCE_CONTINUE;
        });
        
        this._logMessage("Jasper extension timers set up, starting initial refresh");
        
        // Initial refresh after short delay
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 2000, () => {
            this._refreshInsights();
            return GLib.SOURCE_REMOVE;
        });
    }

    disable() {
        this._logMessage("Jasper extension disable() called - cleaning up");
        
        if (this._timeoutId) {
            GLib.Source.remove(this._timeoutId);
            this._timeoutId = null;
        }
        
        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }
        
        this._label = null;
        
        this._logMessage("Jasper extension disabled and cleaned up");
    }

    _refreshInsights() {
        this._logMessage("refreshInsights() called - attempting D-Bus communication");
        
        try {
            // Use gdbus call for reliable D-Bus communication
            const [success, stdout] = GLib.spawn_command_line_sync(
                'gdbus call --session --dest org.personal.CompanionAI --object-path /org/personal/CompanionAI/Companion --method org.personal.CompanionAI.Companion1.GetFormattedInsights "gnome"'
            );
            
            if (success && stdout) {
                const output = new TextDecoder().decode(stdout);
                this._logMessage(`D-Bus call successful, got response: ${output.substring(0, 100)}...`);
                
                // Parse D-Bus response format: ("JSON_STRING",)
                const match = output.match(/\("([^"]+)"/); 
                if (match) {
                    const jsonString = match[1];
                    // Unescape the JSON
                    const unescapedJson = jsonString.replace(/\\"/g, '"').replace(/\\\\/g, '\\');
                    
                    try {
                        const data = JSON.parse(unescapedJson);
                        this._logMessage(`Parsed JSON data successfully, text: ${data.text}`);
                        
                        // Update emoji
                        if (data.text) {
                            this._label.set_text(data.text);
                            this._logMessage(`Updated panel text to: ${data.text}`);
                        }
                        
                        // Update tooltip
                        if (data.tooltip && this._indicator) {
                            this._indicator.set_tooltip_text(data.tooltip);
                            this._logMessage("Updated tooltip");
                        }
                        
                        return;
                    } catch (parseError) {
                        this._logMessage(`JSON parse failed: ${parseError.message}`);
                        // JSON parse failed, fall through to error handling
                    }
                } else {
                    this._logMessage("D-Bus response format didn't match expected pattern");
                }
            } else {
                this._logMessage(`D-Bus call failed: success=${success}, stdout=${stdout ? 'present' : 'null'}`);
            }
        } catch (error) {
            this._logMessage(`Exception during D-Bus call: ${error.message}`);
        }
        
        // Fallback to calendar emoji if daemon not available
        this._logMessage("Using fallback display (daemon not available or failed)");
        this._label.set_text('📅');
        if (this._indicator) {
            this._indicator.set_tooltip_text('Jasper: Waiting for daemon...');
        }
    }

    _logMessage(message) {
        // Multiple logging approaches to ensure visibility
        console.log(`[Jasper Extension] ${message}`);
        
        // Log to file for persistence
        try {
            const logFile = `${GLib.get_home_dir()}/.jasper-extension-dev.log`;
            const timestamp = new Date().toISOString();
            const logEntry = `${timestamp}: ${message}\n`;
            
            GLib.file_set_contents(logFile, logEntry, -1);
        } catch (e) {
            // File logging failed, continue silently
        }
        
        // Also log to journal if possible
        try {
            GLib.spawn_command_line_async(`logger -t jasper-extension "${message}"`);
        } catch (e) {
            // Journal logging failed, continue silently
        }
    }
}