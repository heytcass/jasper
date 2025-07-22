#!/usr/bin/env python3
"""
Dynamic Waybar Configuration Merger for Jasper Development Mode

This script merges your live waybar configuration with the Jasper module definition,
allowing development mode to use your full configuration while enabling Jasper development.
"""

import json
import sys
from pathlib import Path

def merge_waybar_config(live_config_path, jasper_template_path, output_path):
    """
    Merge live waybar config with Jasper module definition.
    
    Args:
        live_config_path: Path to current live waybar config
        jasper_template_path: Path to Jasper's config template  
        output_path: Where to write the merged config
    """
    try:
        # Read live configuration
        with open(live_config_path, 'r') as f:
            live_config = json.load(f)
        
        # Read Jasper template configuration
        with open(jasper_template_path, 'r') as f:
            jasper_template = json.load(f)
        
        # Ensure both are arrays with at least one config object
        if not isinstance(live_config, list) or len(live_config) == 0:
            print(f"Error: Live config should be an array with at least one configuration", file=sys.stderr)
            return False
            
        if not isinstance(jasper_template, list) or len(jasper_template) == 0:
            print(f"Error: Jasper template should be an array with at least one configuration", file=sys.stderr)
            return False
        
        # Work with the first configuration in each
        live_cfg = live_config[0]
        jasper_cfg = jasper_template[0]
        
        # Extract Jasper module definition
        jasper_module = jasper_cfg.get('custom/jasper')
        if not jasper_module:
            print("Error: No custom/jasper module found in template", file=sys.stderr)
            return False
        
        # Create merged configuration
        merged_cfg = live_cfg.copy()
        
        # Add/update the Jasper module
        if 'custom/jasper' not in merged_cfg:
            merged_cfg['custom/jasper'] = jasper_module
        else:
            # Update existing Jasper module with template values
            merged_cfg['custom/jasper'].update(jasper_module)
        
        # Ensure Jasper is in modules-center if it's not already in any modules list
        jasper_in_modules = False
        for modules_key in ['modules-left', 'modules-center', 'modules-right']:
            if modules_key in merged_cfg and 'custom/jasper' in merged_cfg[modules_key]:
                jasper_in_modules = True
                break
        
        if not jasper_in_modules:
            # Add to modules-center, creating it if needed
            if 'modules-center' not in merged_cfg:
                merged_cfg['modules-center'] = []
            if 'custom/jasper' not in merged_cfg['modules-center']:
                merged_cfg['modules-center'].append('custom/jasper')
        
        # Write merged configuration
        merged_config = [merged_cfg]  # Wrap in array
        with open(output_path, 'w') as f:
            json.dump(merged_config, f, indent=2)
        
        print(f"✅ Successfully merged waybar configuration")
        print(f"   Live config: {live_config_path}")
        print(f"   Jasper template: {jasper_template_path}")
        print(f"   Output: {output_path}")
        return True
        
    except json.JSONDecodeError as e:
        print(f"Error: JSON parsing failed - {e}", file=sys.stderr)
        return False
    except FileNotFoundError as e:
        print(f"Error: File not found - {e}", file=sys.stderr)
        return False
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return False

def main():
    if len(sys.argv) != 4:
        print("Usage: merge-waybar-config.py <live_config> <jasper_template> <output>", file=sys.stderr)
        print("", file=sys.stderr)
        print("Example:", file=sys.stderr) 
        print("  merge-waybar-config.py ~/.config/waybar/config.backup waybar/config.json waybar/merged-config.json", file=sys.stderr)
        sys.exit(1)
    
    live_config_path = Path(sys.argv[1])
    jasper_template_path = Path(sys.argv[2])
    output_path = Path(sys.argv[3])
    
    if not live_config_path.exists():
        print(f"Error: Live config file does not exist: {live_config_path}", file=sys.stderr)
        sys.exit(1)
        
    if not jasper_template_path.exists():
        print(f"Error: Jasper template file does not exist: {jasper_template_path}", file=sys.stderr)
        sys.exit(1)
    
    success = merge_waybar_config(live_config_path, jasper_template_path, output_path)
    sys.exit(0 if success else 1)

if __name__ == "__main__":
    main()