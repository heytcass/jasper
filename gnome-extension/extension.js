// Jasper GNOME Shell Extension - Working version with D-Bus
const Main = imports.ui.main;
const PanelMenu = imports.ui.panelMenu;
const {St, Clutter, GLib, Gio} = imports.gi;

// Observable logging function for development verification
function logMessage(message) {
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

let indicator;
let label;
let timeoutId;

function init() {
    // Extension initialized - log to verify execution
    logMessage("Jasper extension init() called");
}

function enable() {
    logMessage("Jasper extension enable() called - creating UI elements");
    
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
    
    logMessage("Jasper extension UI created and added to panel");
    
    // Set up refresh timer - check every 5 seconds
    timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 5000, () => {
        refreshInsights();
        return GLib.SOURCE_CONTINUE;
    });
    
    logMessage("Jasper extension timers set up, starting initial refresh");
    
    // Initial refresh after short delay
    GLib.timeout_add(GLib.PRIORITY_DEFAULT, 2000, () => {
        refreshInsights();
        return GLib.SOURCE_REMOVE;
    });
}

function disable() {
    logMessage("Jasper extension disable() called - cleaning up");
    
    if (timeoutId) {
        GLib.Source.remove(timeoutId);
        timeoutId = null;
    }
    
    if (indicator) {
        indicator.destroy();
        indicator = null;
    }
    
    label = null;
    
    logMessage("Jasper extension disabled and cleaned up");
}

function refreshInsights() {
    logMessage("refreshInsights() called - attempting D-Bus communication");
    
    try {
        // Use gdbus call for reliable D-Bus communication
        const [success, stdout] = GLib.spawn_command_line_sync(
            'gdbus call --session --dest org.personal.CompanionAI --object-path /org/personal/CompanionAI/Companion --method org.personal.CompanionAI.Companion1.GetFormattedInsights "gnome"'
        );
        
        if (success && stdout) {
            const output = new TextDecoder().decode(stdout);
            logMessage(`D-Bus call successful, got response: ${output.substring(0, 100)}...`);
            
            // Parse D-Bus response format: ("JSON_STRING",)
            const match = output.match(/\("([^"]+)"/);
            if (match) {
                const jsonString = match[1];
                // Unescape the JSON
                const unescapedJson = jsonString.replace(/\\"/g, '"').replace(/\\\\/g, '\\');
                
                try {
                    const data = JSON.parse(unescapedJson);
                    logMessage(`Parsed JSON data successfully, text: ${data.text}`);
                    
                    // Update emoji
                    if (data.text) {
                        label.set_text(data.text);
                        logMessage(`Updated panel text to: ${data.text}`);
                    }
                    
                    // Update tooltip
                    if (data.tooltip && indicator) {
                        indicator.set_tooltip_text(data.tooltip);
                        logMessage("Updated tooltip");
                    }
                    
                    return;
                } catch (parseError) {
                    logMessage(`JSON parse failed: ${parseError.message}`);
                    // JSON parse failed, fall through to error handling
                }
            } else {
                logMessage("D-Bus response format didn't match expected pattern");
            }
        } else {
            logMessage(`D-Bus call failed: success=${success}, stdout=${stdout ? 'present' : 'null'}`);
        }
    } catch (error) {
        logMessage(`Exception during D-Bus call: ${error.message}`);
    }
    
    // Fallback to calendar emoji if daemon not available
    logMessage("Using fallback display (daemon not available or failed)");
    label.set_text('📅');
    if (indicator) {
        indicator.set_tooltip_text('Jasper: Waiting for daemon...');
    }
}