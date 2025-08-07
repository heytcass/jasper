// Jasper GNOME Shell Extension - Working version with D-Bus
const Main = imports.ui.main;
const PanelMenu = imports.ui.panelMenu;
const {St, Clutter, GLib, Gio} = imports.gi;

let indicator;
let label;
let timeoutId;

function init() {
    // Extension initialized
}

function enable() {
    // Create indicator
    indicator = new PanelMenu.Button(0.0, 'Jasper AI Insights', false);
    
    // Create label with initial loading emoji
    label = new St.Label({
        text: '🔄',
        style_class: 'system-status-icon',
        y_align: Clutter.ActorAlign.CENTER,
        x_align: Clutter.ActorAlign.CENTER,
    });
    
    indicator.add_child(label);
    Main.panel.addToStatusArea('jasper-ai-insights', indicator);
    
    // Set up refresh timer - check every 5 seconds
    timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 5000, () => {
        refreshInsights();
        return GLib.SOURCE_CONTINUE;
    });
    
    // Initial refresh after short delay
    GLib.timeout_add(GLib.PRIORITY_DEFAULT, 2000, () => {
        refreshInsights();
        return GLib.SOURCE_REMOVE;
    });
}

function disable() {
    if (timeoutId) {
        GLib.Source.remove(timeoutId);
        timeoutId = null;
    }
    
    if (indicator) {
        indicator.destroy();
        indicator = null;
    }
    
    label = null;
}

function refreshInsights() {
    try {
        // Use gdbus call for reliable D-Bus communication
        const [success, stdout] = GLib.spawn_command_line_sync(
            'gdbus call --session --dest org.personal.CompanionAI --object-path /org/personal/CompanionAI/Companion --method org.personal.CompanionAI.Companion1.GetFormattedInsights "gnome"'
        );
        
        if (success && stdout) {
            const output = new TextDecoder().decode(stdout);
            
            // Parse D-Bus response format: ("JSON_STRING",)
            const match = output.match(/\("([^"]+)"/);
            if (match) {
                const jsonString = match[1];
                // Unescape the JSON
                const unescapedJson = jsonString.replace(/\\"/g, '"').replace(/\\\\/g, '\\');
                
                try {
                    const data = JSON.parse(unescapedJson);
                    
                    // Update emoji
                    if (data.text) {
                        label.set_text(data.text);
                    }
                    
                    // Update tooltip
                    if (data.tooltip && indicator) {
                        indicator.set_tooltip_text(data.tooltip);
                    }
                    
                    return;
                } catch (parseError) {
                    // JSON parse failed, fall through to error handling
                }
            }
        }
    } catch (error) {
        // D-Bus call failed
    }
    
    // Fallback to calendar emoji if daemon not available
    label.set_text('📅');
    if (indicator) {
        indicator.set_tooltip_text('Jasper: Waiting for daemon...');
    }
}