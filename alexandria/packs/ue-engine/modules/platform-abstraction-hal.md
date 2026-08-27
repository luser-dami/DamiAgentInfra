---
name: platform-abstraction-hal
description: The platform abstraction layer — GenericPlatform vs WindowsPlatform split, what lives in the HAL, and the rules for writing code that must compile everywhere.
module: platform-abstraction-hal
---

# Platform Abstraction / HAL (UE5 Expert)

## Context
- Read before writing `#if PLATFORM_*` code, touching threads/atomics/file IO, or porting to a new platform.
- The HAL exists so that engine code says `FPlatformMisc::` and never `Windows.h`.

## 1. The Shape

```
FGenericPlatform* (defaults for every platform)
        │
        ▼
FWindowsPlatform* / FUnixPlatform* / FApplePlatform* (overrides)
        │
        ▼
FPlatform* typedef = the current platform's concrete struct
```

Every platform service — memory stats, atomics, TLS, time, file paths, process, misc — is a static struct in this hierarchy. Engine code always calls through the `FPlatform*` typedef, which resolves at compile time; there is no virtual dispatch.

## 2. What's Actually Down There

| Service | Entry | Notes |
|---------|-------|-------|
| Memory | `FPlatformMemory` / `FGenericPlatformMemory` | Stats, allocation hooks, page sizes |
| Atomics | `FPlatformAtomics` (`GenericPlatformAtomics.h`) | Lock-free primitives; use `std::atomic` in new code where possible |
| Misc | `FWindowsPlatformMisc` etc. | CPU info, GUIDs, error dialogs, env vars |
| Time | `FPlatformTime` | High-resolution clocks |
| File IO | `IFileManager` / `FPlatformFileManager` | Async-friendly, pak-aware |

## 3. The Compile-Time Contract

Platform selection happens via `Platform.h` precompiled headers injected per module: `PLATFORM_WINDOWS` et al. are literals, so `#if PLATFORM_WINDOWS` branches are free. Code must keep platform ifdefs **small and at the bottom** — the platform layer absorbs OS differences so game code doesn't accumulate them. A growing `#if` count in gameplay code is a design smell pointing at a missing HAL service.

## Verification Checklist

- Build breaks on one platform only: you used an OS API directly — route through the matching FPlatform* service or add one.
- Timing differs across machines: use `FPlatformTime::Seconds`, not raw QPC/RDTSC.
- Cross-platform path bugs: never hand-concatenate paths — `FPaths` and `IFileManager` exist for this.

## Architecture

The HAL is compile-time polymorphism by typedef and macro, not runtime abstraction: zero cost, full inlining, and forced upfront handling of platform differences. Keeping OS calls out of gameplay code is the entire point — once `Windows.h` leaks past the HAL boundary, portability is already lost.

## Data Flow

```
Engine code: FPlatformMisc:: / FPlatformMemory:: / FPlatformTime::
        │
        ▼ (compile-time typedef)
FWindowsPlatformMisc (current platform's struct)
        │
        ▼
Win32 / POSIX / platform SDK
```

## Key Claims

- [extracted] `FWindowsPlatformMisc` is defined at `Source/Runtime/Core/Public/Windows/WindowsPlatformMisc.h` and is the Windows concrete implementation of the platform-misc service.
- [extracted] `FGenericPlatformMemoryConstants` is defined at `Source/Runtime/Core/Public/GenericPlatform/GenericPlatformMemory.h` and provides the defaults every platform overrides.
- [inferred] Compile-time typedefs instead of virtuals keep HAL calls free of indirection, which is why atomics and time can live behind them.

## Edge Cases

- `PLATFORM_HAS_*` feature macros differ from `PLATFORM_*` identity macros — capability checks should use the former.
- Editor-only platform services (dialogs, clipboards) are stubbed in server/monolithic builds — guard accordingly.
- Console platform specifics live under restricted trees not present in a public install.

## Boundaries

- The HAL covers OS primitives only; rendering API differences (D3D/Vulkan/Metal) belong to the RHI layer.
- GenericPlatform fallbacks are correct-but-slow defaults, not reference implementations to copy.

## Evidence

- `FWindowsPlatformMisc` defined at `Source/Runtime/Core/Public/Windows/WindowsPlatformMisc.h`
- `FGenericPlatformMemoryConstants` defined at `Source/Runtime/Core/Public/GenericPlatform/GenericPlatformMemory.h`
- `FGenericPlatformMemoryStats` defined at `Source/Runtime/Core/Public/GenericPlatform/GenericPlatformMemory.h`
