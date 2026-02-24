# OBServe Distribution Plan

Free app with gated paid modules ($1.99 each via Stripe).

## Distribution Options

### 1. GitHub Releases (Easiest start)
- Already have `smythmyke/OBServe` on GitHub
- Tauri builds produce `.msi` / `.exe` installers — attach to a GitHub Release
- Free, no approval process, instant
- OBS community is GitHub-native — they'll trust it more
- Downside: no built-in discovery, need to drive traffic

### 2. Gumroad
- Supports "pay what you want" (set $0 minimum for free)
- Handles downloads, email collection, and update notifications
- Module purchases still go through existing Stripe + Cloudflare Worker
- 10% fee on paid transactions — free tier costs nothing
- Good landing page out of the box
- Downside: another platform to maintain, slight brand dilution

### 3. Own Website
- Full control over branding, messaging, SEO
- Host on Cloudflare Pages (free) — already have a Worker there
- Link directly to GitHub Releases for downloads
- Module purchases go through existing Stripe flow
- Downside: takes time to build, responsible for trust signals (SSL, design, etc.)

### 4. itch.io
- Strong indie/creator community, supports free desktop apps
- Built-in app installer (itch desktop client) with auto-updates
- 0% minimum revenue share (you choose)
- Good discoverability among creators
- Downside: more gaming-focused, OBS users may not look there

### 5. Microsoft Store
- Tauri/MSIX packaging is supported but adds build complexity
- Legitimacy boost, auto-updates via Store
- Downside: review process, 15% cut on in-app purchases, slow iteration

## Recommended Rollout

### Phase 1 — Now
- **GitHub Releases** + simple landing page on **Cloudflare Pages**
- Zero cost, live immediately
- Landing page: what it does, screenshot, download button → GitHub Releases, module store link

### Phase 2 — Beta Traction
- Add **Gumroad** as secondary channel (price set to $0)
- Email collection for update announcements
- Broader audience

### Phase 3 — If Demand Warrants
- Polish website into a proper product page
- Consider **Microsoft Store** for legitimacy

## Marketing Channels
- r/obs subreddit
- OBS Forums
- Streaming communities on Discord
- YouTube creator communities

## Notes
- Module purchases go through Stripe via Cloudflare Worker — platform-independent
- Distribution channel is really just about getting the free installer into hands
- GitHub Release + landing page is enough credibility for OBS community early adopters
