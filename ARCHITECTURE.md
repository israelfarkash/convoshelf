# Architecture

## Overview

WhatsApp Export Viewer is a Tauri 2 desktop app. React renders the Hebrew RTL interface, while Rust owns ZIP extraction, parsing, SQLite persistence, and local media reads. No chat content is sent over the network.

## Runtime flow

1. The user selects or drops a WhatsApp `.zip` export.
2. `src-tauri/src/importer.rs` validates and extracts entries into a unique app-data directory.
3. The importer parses the chat text and writes chats and messages to SQLite in one transaction.
4. `src/App.tsx` requests chats, messages, and statistics through narrow Tauri commands.
5. `src/MediaDisplay.tsx` displays media through Tauri's local asset protocol, whose scope is restricted to the app's imports directory.

## Modules

- `src/App.tsx`: application state, chat/message rendering, search, import, and local edit actions.
- `src/MediaDisplay.tsx`: lazy local media rendering through Tauri's scoped asset protocol.
- `src/index.css`: all visual styling, including a locally generated chat background pattern.
- `src-tauri/src/importer.rs`: safe ZIP extraction, WhatsApp text parsing, and media encoding.
- `src-tauri/src/database.rs`: schema initialization and SQLite queries/updates.
- `src-tauri/src/models.rs`: values serialized from Rust to the frontend.
- `src-tauri/src/lib.rs`: Tauri plugins, command registration, and database startup.

## Storage

Tauri resolves the platform app-data directory. The app stores:

```text
database.sqlite
imports/<unique-import-id>/...
```

The selected source archive is read only and is never modified. Edits and deletions affect only the local SQLite copy and preserve original message text for restoration.

Existing installations that used the legacy `com.whatsappexportviewer.app` identifier continue to use that data directory automatically, so upgrades do not duplicate or hide imported chats.

## Offline and security boundaries

- Production CSP permits only bundled resources, data/blob media, and Tauri IPC.
- The app has dialog permission but no frontend filesystem permission.
- ZIP paths must pass `enclosed_name`, preventing absolute paths and parent traversal.
- Archive file count and extracted size are limited.
- Failed imports remove their partial extraction directory and roll back their database transaction.
- The asset protocol can read only the current imports directory and the exact legacy macOS imports directory used by earlier releases.
