# Still

A proactive, local-first Windows session optimizer and recoverable storage assistant built with Tauri 2, React, TypeScript and Rust.

Still explains concrete session pressure, shows protected process actions, scans selected user folders without following reparse points, and moves confirmed cleanup selections to the Windows Recycle Bin.

Adaptive autopilot continuously observes CPU and memory pressure. It can temporarily lower validated background processes from Normal to Below Normal priority and ask Windows to release their unused working-set pages, then restores priorities when pressure clears. System processes, the foreground application and its descendants, Docker, WSL, Android Studio and active developer workloads are excluded.

Optional Deep close remembers eligible apps that had a visible window. After the last window closes, it allows a short normal-exit grace period and then stops only verified leftovers from that app's recorded process tree. It never requests UAC automatically and excludes Windows, VPN connections, Docker, WSL, Android Studio, developer tools and user-marked expected background apps.

“Optimize now” reports measured RAM released. Explicit process actions are verified after termination and can request one scoped UAC approval when Windows requires it. Calm-time maintenance can move aged files from an allow-list of user Temp, CrashDumps and DirectX cache folders to the Recycle Bin once per day. Browser and developer caches are discoverable but always remain manual selections; permanent deletion is absent.

The bundled LibreHardwareMonitor-based helper supplies supported CPU, GPU, storage, clock, fan, power and VRAM readings through a local cache. It runs with standard permissions by default. Enhanced access can elevate only the helper for the current Still session; invalid zero-temperature readings are never shown as real values. Storage temperatures identify the physical drive and sensor that reported the hottest valid reading; they are not presented as the temperature of the whole PC.

See `AGENTS.md` for architecture, commands and safety invariants.
