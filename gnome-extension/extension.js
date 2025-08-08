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
        
        // Store tooltip text
        this._tooltipText = 'Jasper: Loading...';
        
        // Add hover tooltip functionality
        this._indicator.reactive = true;
        this._indicator.track_hover = true;
        
        let tooltip = null;
        this._indicator.connect('notify::hover', (actor) => {
            if (actor.hover) {
                // Show tooltip on hover
                if (!tooltip) {
                    tooltip = new St.Label({
                        style_class: 'dash-label',
                        text: this._tooltipText,
                        opacity: 0,
                        style: 'padding: 6px 12px; background-color: rgba(0,0,0,0.9); color: white; border-radius: 5px;'
                    });
                    Main.uiGroup.add_child(tooltip);
                }
                
                tooltip.set_text(this._tooltipText);
                
                // Position tooltip above the indicator
                let [stageX, stageY] = this._indicator.get_transformed_position();
                let [width, height] = this._indicator.get_size();
                tooltip.set_position(stageX + width/2 - tooltip.width/2, stageY - tooltip.height - 5);
                
                tooltip.ease({
                    opacity: 255,
                    duration: 150,
                    mode: Clutter.AnimationMode.EASE_OUT_QUAD
                });
            } else {
                // Hide tooltip
                if (tooltip) {
                    tooltip.ease({
                        opacity: 0,
                        duration: 150,
                        mode: Clutter.AnimationMode.EASE_OUT_QUAD,
                        onComplete: () => {
                            if (tooltip) {
                                Main.uiGroup.remove_child(tooltip);
                                tooltip = null;
                            }
                        }
                    });
                }
            }
        });
        
        // Add click functionality
        this._indicator.connect('button-press-event', () => {
            // Refresh insights on click
            this._refreshInsights();
            return Clutter.EVENT_STOP;
        });
        
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
                console.log('[Jasper] D-Bus output:', JSON.stringify(output));
                
                // D-Bus returns: ("JSON_STRING",)
                // Extract the JSON string from the D-Bus tuple format
                let jsonStr = null;
                
                // Pattern: ("JSON_STRING",) - capture everything between the quotes
                const match = output.match(/^\("(.*)"\s*,?\s*\)$/s);
                if (match) {
                    jsonStr = match[1];
                    console.log('[Jasper] Extracted JSON string:', JSON.stringify(jsonStr));
                    
                    // D-Bus escapes quotes and newlines - convert back to proper JSON
                    console.log('[Jasper] Before unescape - first 50 chars:', JSON.stringify(jsonStr.substring(0, 50)));
                    jsonStr = jsonStr.replace(/\\"/g, '"')  // Unescape quotes
                                    .replace(/\\n/g, '\n')  // Unescape newlines
                                    .replace(/\\\\/g, '\\'); // Unescape backslashes
                    
                    console.log('[Jasper] After unescape - first 50 chars:', JSON.stringify(jsonStr.substring(0, 50)));
                    console.log('[Jasper] Cleaned JSON string length:', jsonStr.length);
                    
                    try {
                        const data = JSON.parse(jsonStr);
                        console.log('[Jasper] Parsed data:', data);
                        
                        if (data.text) {
                            this._label.set_text(data.text);
                            console.log('[Jasper] Set text to:', data.text);
                        }
                        if (data.tooltip) {
                            this._tooltipText = data.tooltip;
                            console.log('[Jasper] Set tooltip to:', data.tooltip);
                        }
                        return;
                    } catch (parseError) {
                        console.warn('[Jasper] JSON parse failed:', parseError.message, 'JSON:', jsonStr);
                    }
                } else {
                    console.warn('[Jasper] Failed to match D-Bus format. Output:', JSON.stringify(output));
                }
            } else {
                console.warn('[Jasper] D-Bus call failed or empty output. Success:', success, 'stderr:', stderr ? new TextDecoder().decode(stderr) : 'none');
            }
        } catch (error) {
            console.warn('[Jasper] D-Bus call exception:', error.message);
        }
        
        // Fallback
        this._label.set_text('📅');
        this._tooltipText = 'Jasper: Waiting for daemon...';
    }
}