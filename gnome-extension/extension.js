// Legacy GNOME Shell extension format for maximum compatibility
const { St, Clutter } = imports.gi;
const Main = imports.ui.main;
const PanelMenu = imports.ui.panelMenu;

let indicator;
let label;

function init() {
    log('[Jasper] Extension init() called');
}

function enable() {
    log('[Jasper] Extension enable() called');
    
    try {
        // Create indicator
        indicator = new PanelMenu.Button(0.0, 'Jasper Test', false);
        
        // Create simple label
        label = new St.Label({
            text: '🧪',
            style_class: 'system-status-icon',
            y_align: Clutter.ActorAlign.CENTER,
        });
        
        indicator.add_child(label);
        Main.panel.addToStatusArea('jasper-test', indicator);
        
        log('[Jasper] Extension UI created successfully');
    } catch (error) {
        log('[Jasper] Error in enable(): ' + error);
    }
}

function disable() {
    log('[Jasper] Extension disable() called');
    
    if (indicator) {
        indicator.destroy();
        indicator = null;
    }
    
    label = null;
    log('[Jasper] Extension disabled successfully');
}