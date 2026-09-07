# Still / Session Lens engineering guide

## Product

Still is a local-first Windows session monitor and safe storage assistant. It explains why the current session feels slow and offers reversible, user-confirmed actions. It never invents health scores or hardware sensor values.

## Architecture

- `src/`: React/TypeScript UI only. It consumes typed commands from `src/lib/native.ts` and never talks to Win32 directly.
- `src-tauri/src/core/telemetry.rs`: live CPU, memory, pagefile, disk and session pressure.
- `src-tauri/src/core/processes.rs`: process inventory, visible-window state, protection policy and explicit process actions.
- `src-tauri/src/core/disk_analysis.rs`: background metadata-first scanning, semantic grouping, progress and cancellation.
- `src-tauri/src/core/cleanup.rs`: canonical path validation, dry-run and Recycle Bin actions.
- `src-tauri/src/core/maintenance.rs`: allow-listed aged temp/cache analysis and recoverable cleanup.
- `src-tauri/src/core/optimizer.rs`: pressure response, priority balancing and safe working-set trimming.
- `src-tauri/src/core/hardware.rs`: lifecycle, cache validation and scoped UAC for the bundled sensor helper.
- `src-tauri/src/core/deep_close.rs`: tracks the last visible window of eligible apps and safely stops verified leftover process groups after a grace period.
- `sensor-helper/`: LibreHardwareMonitor-based .NET helper; Still itself remains unelevated.
- `src-tauri/src/core/recommendations.rs`: explainable recommendations derived from concrete metrics.
- `src-tauri/src/core/safety.rs`: protected process/path policy shared by every destructive command.
- `src-tauri/src/lib.rs`: the typed Tauri command boundary and application state.

## Commands

```powershell
pnpm install
pnpm dev
pnpm tauri dev
pnpm format
pnpm lint
pnpm typecheck
pnpm test
pnpm build
pnpm tauri build
```

Rust-only checks:

```powershell
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Design rules

- Wix Madefor Display is the primary face; technical values may use Geist Mono.
- Near-white environment, floating white panels, large radii, soft shadows, near-black selected states.
- Prefer one strong composition over a grid of identical dashboard cards.
- Motion communicates state: counters, resource rails, scan progress, expansion and completed actions.
- Status is `Calm`, `Busy` or `Pressure` and always includes concrete reasons.
- No fake temperature/GPU values. Unsupported sensors render as `Unavailable` with the adapter reason.
- Treat zero/impossible sensor readings as unavailable. Elevated sensor access applies only to the helper process.

## Safety rules

- Least privilege; no permanent elevation.
- Never concatenate user input into shell commands.
- Canonicalize all paths and reject protected roots and reparse points.
- Never follow symlinks/junctions during scanning.
- Automatic cleanup is opt-in and limited to allow-listed aged user temp, crash-dump and graphics-cache locations; Recycle Bin only. Personal files, browser caches and developer caches always require explicit selection.
- Permanent delete is intentionally absent from the MVP.
- Never terminate protected/system processes. Tree termination validates every member first.
- Automatic Deep close never requests UAC and always excludes Windows, VPN connections, Docker, WSL, Android Studio, developer tools and user-marked expected background apps.
- Logs contain action type, counts and byte totals, never full sensitive paths.
- No analytics, cloud calls or telemetry uploads.

## Definition of Done

- Frontend format, lint, typecheck, tests and production build pass.
- Rust format, clippy and tests pass.
- Native Windows build produces an executable.
- Live telemetry and process refresh work without blocking the UI.
- Protected process termination is rejected and covered by tests.
- Process termination reports success only after Windows confirms the requested PID exited; UAC fallback is scoped to the validated numeric PID.
- Storage scan reports progress, can be cancelled and groups meaningful candidates.
- Cleanup dry-run matches the confirmed Recycle Bin action.
- The app launches without runtime console errors.
