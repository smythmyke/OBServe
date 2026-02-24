# OBServe Distribution Guide

## Overview

OBServe ships as a free Windows desktop app with paid modules ($1.99 each via Stripe).

- **Gumroad** — distributes the free installer (handles downloads, email collection, update announcements)
- **In-app Stripe** — handles module purchases (Payment Links → Cloudflare Worker → Ed25519 license key)
- **GitHub Releases** — hosts update artifacts (NSIS installer + `latest.json` for auto-updater)

## Release Workflow

### 1. Bump Version

Update the version in all three files:
- `package.json` → `"version": "X.Y.Z"`
- `src-tauri/Cargo.toml` → `version = "X.Y.Z"`
- `src-tauri/tauri.conf.json` → `"version": "X.Y.Z"`

### 2. Commit & Tag

```bash
git add -A && git commit -m "Release vX.Y.Z"
git tag vX.Y.Z
git push origin main --tags
```

### 3. GitHub Actions Builds

Pushing a `v*` tag triggers `.github/workflows/release.yml`:
- Builds Windows NSIS installer via `tauri-apps/tauri-action`
- Creates a **draft** GitHub Release with:
  - `OBServe_X.Y.Z_x64-setup.exe` (NSIS installer)
  - `latest.json` (auto-updater manifest)

### 4. Review & Publish

1. Go to GitHub Releases → find the draft
2. Edit release notes (changelog, highlights)
3. Publish the release

### 5. Upload to Gumroad

1. Download the `.exe` installer from the published release
2. Go to Gumroad product page → update the deliverable file
3. (Optional) Send update announcement to customers

## Gumroad Setup

### Product Page

1. Create product at gumroad.com
2. Set price to **$0+** (free with optional tip)
3. Add product description, screenshots, feature list
4. Upload the NSIS installer (`.exe`) as the deliverable
5. Enable "Email me when someone gets this"

### Deliverable Settings

- Deliverable type: Digital download
- File: `OBServe_X.Y.Z_x64-setup.exe`
- Update the file each release

## Auto-Updater

### Generate Signing Key

```bash
npx tauri signer generate -w ~/.tauri/OBServe.key
```

This creates:
- `~/.tauri/OBServe.key` — private key (keep secret)
- `~/.tauri/OBServe.key.pub` — public key

### Configure

1. Copy the public key into `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`
2. Set GitHub Actions secrets:
   - `TAURI_SIGNING_PRIVATE_KEY` — contents of `~/.tauri/OBServe.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the password you chose (or empty string)

### How It Works

- App checks `https://github.com/smythmyke/OBServe/releases/latest/download/latest.json` 5 seconds after startup
- If a newer version exists, prompts the user to download and install
- Update is downloaded, verified against the public key, installed, and app relaunches

## Code Signing (Windows)

### Why

Without code signing, Windows SmartScreen will show "Windows protected your PC" when users run the installer. This doesn't block installation but reduces trust.

### Options

| Option | Cost | SmartScreen Impact |
|--------|------|-------------------|
| No cert | Free | Warning on every install until reputation builds |
| OV cert | ~$200-400/yr | Warning initially, clears after reputation (~500 installs) |
| EV cert | ~$400-600/yr | Immediate SmartScreen bypass |

### Implementation

1. Purchase an EV code signing cert (DigiCert, Sectigo, SSL.com)
2. Set `TAURI_SIGNING_IDENTITY` environment variable with the cert subject name
3. The Tauri build process will automatically sign the installer

### Recommended Providers

- **SSL.com** — cheapest EV certs (~$400/yr), supports cloud signing
- **DigiCert** — most widely trusted, higher price
- **Sectigo** — mid-range pricing

## Stripe Configuration

### Per-Module Products

For each module, create a Stripe Product:
1. Go to Stripe Dashboard → Products → Add product
2. Set name (e.g., "OBServe — Pro Spectrum")
3. Set price: $1.99 one-time
4. Add metadata: `observe_module_id` = `spectrum` (module ID)
5. Create a Payment Link for each product
6. Update the `stripe_link` in `src-tauri/src/store.rs`

### Bundle Product

1. Create "OBServe — All Modules Bundle" product
2. Price: $9.99 one-time
3. Metadata: `observe_module_id` = `all-modules-bundle`
4. The worker automatically expands this to all 10 module IDs

### Webhook

1. Go to Stripe Dashboard → Developers → Webhooks
2. Add endpoint: `https://observe-api.smythmyke.workers.dev/webhook`
3. Select events: `checkout.session.completed`, `charge.refunded`
4. Copy the signing secret
5. Set via `wrangler secret put STRIPE_WEBHOOK_SECRET`

## Cloudflare Worker Secrets

```bash
wrangler secret put STRIPE_SECRET_KEY        # sk_live_...
wrangler secret put ED25519_PRIVATE_KEY_HEX  # 64-char hex
wrangler secret put STRIPE_WEBHOOK_SECRET    # whsec_...
wrangler secret put ADMIN_PIN                # 4-digit admin code
wrangler secret put RESEND_API_KEY           # re_...
```

### KV Namespace (Rate Limiting)

```bash
wrangler kv namespace create RATE_LIMIT_KV
# Copy the returned ID into wrangler.toml
```

## Email Delivery (Resend)

1. Create account at resend.com (free: 3,000 emails/month)
2. Verify domain `observe.app` (or use `onboarding@resend.dev` for testing)
3. Get API key → set via `wrangler secret put RESEND_API_KEY`
4. Emails are sent automatically on purchase (webhook) and license recovery

## Module IDs

| Module ID | Name | Price |
|-----------|------|-------|
| `spectrum` | Pro Spectrum | $1.99 |
| `video-editor` | Video Review & Editor | $1.99 |
| `calibration` | Audio Calibration | $1.99 |
| `ducking` | Sidechain Ducking + Mixer | $1.99 |
| `audio-fx` | Airwindows VST Plugins | $1.99 |
| `camera` | Camera Scene Auto-Detect | $1.99 |
| `presets` | Smart Presets | $1.99 |
| `narration-studio` | Narration Studio | $1.99 |
| `monitoring` | Advanced Monitoring | $1.99 |
| `sample-pad` | OBServe Pads | $1.99 |
| `all-modules-bundle` | All Modules Bundle | $9.99 |
