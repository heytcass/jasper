# Cachix Setup for Jasper Companion

## Quick User Setup

Add the Jasper Companion binary cache to your system:

```bash
# Add the cache (do this once)
cachix use jasper-companion
```

Or manually add to your `configuration.nix`:

```nix
{
  nix = {
    settings = {
      substituters = [
        "https://jasper-companion.cachix.org"
        "https://cache.nixos.org/"
      ];
      trusted-public-keys = [
        "jasper-companion.cachix.org-1:YOUR_PUBLIC_KEY_HERE"
        "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
      ];
    };
  };
}
```

## Developer Setup (for contributors)

### 1. Create Cachix Cache

```bash
# Install cachix if not already installed
nix-env -iA cachix -f https://cachix.org/api/v1/install

# Create your cache (do this once)
cachix authtoken YOUR_AUTH_TOKEN
cachix create jasper-companion
```

### 2. Add GitHub Secret

Add `CACHIX_AUTH_TOKEN` to your GitHub repository secrets with your cachix auth token.

### 3. Local Development with Cachix

```bash
# Use the cache locally
cachix use jasper-companion

# Your builds will now use cached binaries when available
nix build .#daemon
nix build .#gnome-extension
```

## How It Works

1. **CI Builds**: Every push to main and PRs trigger builds
2. **Auto-Push**: Successfully built packages are automatically pushed to cachix
3. **Fast Installs**: Users get pre-built binaries instead of compiling from source
4. **Smart Filtering**: Only pushes meaningful packages (not source tarballs)

## Cache Contents

The cache will contain:
- `jasper-companion-daemon` binary
- `jasper-companion-gnome-extension` package
- Development shell dependencies
- All Rust dependencies and build artifacts

## Benefits

- **Users**: Fast installation (seconds instead of minutes)
- **Developers**: Faster CI and local development
- **CI**: Reduced compute time and costs
- **Community**: Easier adoption and testing