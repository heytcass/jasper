// Minimal test extension to debug GNOME Shell 48 compatibility
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import * as Extension from 'resource:///org/gnome/shell/extensions/extension.js';

export default class JasperExtension extends Extension.Extension {
    constructor(...args) {
        super(...args);
        this._indicator = null;
        this._label = null;
    }

    enable() {
        console.log('[Jasper] Extension enable() called');
        
        try {
            // Create indicator
            this._indicator = new PanelMenu.Button(0.0, 'Jasper Test', false);
            
            // Create simple label
            this._label = new St.Label({
                text: '🧪',
                style_class: 'system-status-icon',
                y_align: Clutter.ActorAlign.CENTER,
            });
            
            this._indicator.add_child(this._label);
            Main.panel.addToStatusArea('jasper-test', this._indicator);
            
            console.log('[Jasper] Extension UI created successfully');
        } catch (error) {
            console.error('[Jasper] Error in enable():', error);
        }
    }

    disable() {
        console.log('[Jasper] Extension disable() called');
        
        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }
        
        this._label = null;
        console.log('[Jasper] Extension disabled successfully');
    }
}