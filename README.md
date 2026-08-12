# WhatsApp Export Viewer

A private desktop app for reading exported WhatsApp chats on macOS. Import a WhatsApp `.zip` export, browse messages and media, search locally, and make reversible local edits without changing the original archive.

> This is an independent project and is not affiliated with or endorsed by WhatsApp or Meta.

## Privacy and offline use

- The packaged app works fully offline.
- Chat data and extracted media stay in the app's local data directory.
- There are no analytics, telemetry, cloud sync, remote fonts, or remote images.
- The production Content Security Policy blocks external network connections.
- ZIP entries are validated before extraction, and media access is restricted to the app's imports directory.

## Features

- Import WhatsApp `.zip` exports by file picker or drag and drop.
- Parse Android and iOS text export formats, including multiline and system messages.
- Display images, stickers, videos, audio, and document attachments.
- Search messages and filter imported chats.
- Rename chats and locally edit, delete, or restore messages.
- Render large conversations efficiently with a virtualized message list.
- Hebrew RTL interface with light and dark appearance support.

## Development

Requirements: Node.js, npm, Rust, and the platform prerequisites for Tauri 2.

```bash
npm install
npm run tauri dev
```

Useful checks:

```bash
npm run lint
npm run build
cargo check --manifest-path src-tauri/Cargo.toml --locked
```

Create the macOS app and DMG:

```bash
npm run tauri build
```

## Project structure

```text
src/                    React and TypeScript interface
src-tauri/src/          Rust importer, database, and Tauri commands
src-tauri/capabilities/ Minimal desktop permissions
public/                 Bundled local static assets
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for implementation details.
