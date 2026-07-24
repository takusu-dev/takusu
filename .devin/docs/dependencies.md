# Workspace Dependencies

| Crate | Version | Used by | Notes |
|-------|---------|---------|-------|
| `thiserror` | 2.0 | workspace | Error derive macro |
| `jiff` | 0.2.21 | takusu-core, takusu-local, takusu-cli | Date/time handling |
| `rand` | 0.10 | takusu-core | RNG for SA |
| `rustc-hash` | 2.1 | takusu-core | `FxHashSet` / `FxHashMap` (faster than std) |
| `rayon` | 1.10 | takusu-core | Parallel SA restarts |
| `criterion` | 5.0.1 (`codspeed-criterion-compat`) | takusu-core, takusu-habit, takusu-ical (dev) | CodSpeed-compatible benchmarking |
| `tokio` | 1.52.0 | workspace | Async runtime (full features) |
| `axum` | 0.8 | takusu-local | HTTP framework |
| `sqlx` | 0.9 (sqlite) | takusu-local, takusu-local-lib | SQLite async driver |
| `serde` / `serde_json` | 1 / 1 | takusu-local, takusu-ical, takusu-client | Serialization |
| `uuid` | 1 (v7) | takusu-local, takusu-local-lib | ID generation |
| `sha2` | 0.11 | takusu-local-lib | Token hashing |
| `tower-http` | 0.7 (cors,trace) | takusu-local | HTTP middleware |
| `tracing` / `tracing-subscriber` | 0.1 / 0.3 | takusu-local, takusu-local-lib | Logging |
| `async-trait` | 0.1 | takusu-local-lib | Async trait |
| `reqwest` | 0.13 (rustls) | google-cal, takusu-local-lib, takusu-client, takusu-audio | HTTP client |
| `clap` | 4 (derive,env) | takusu-cli | CLI argument parsing |
| `comfy-table` | 7 | takusu-cli | Rich table display |
| `cpal` | 0.18.1 | takusu-audio | Audio input |
| `futures-util` | 0.3 | takusu-audio | Async stream utilities |
