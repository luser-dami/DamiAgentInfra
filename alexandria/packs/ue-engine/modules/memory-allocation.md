---
name: memory-allocation
description: Unreal's memory layer — FMemory's routing, the FMalloc allocator family (binned, ANSI, platform), FMemStack-style frame allocators, and how to choose between them.
module: memory-allocation
---

# Memory Allocation Internals (UE5 Expert)

## Context
- Read before optimizing allocation-heavy code, debugging memory growth, or writing container-heavy hot paths.
- Most "Unreal is slow" profiling ends in this layer: allocation count, not allocator speed, is usually the problem.

## 1. The Stack

```
FMemory static API (Malloc/Free/Realloc/Memcpy...)
        │
        ▼
GMalloc (process-global FMalloc implementation)
        ├─ Binned allocators (FMallocBinned2/3) — default on desktop: size-class pools
        ├─ ANSI malloc — CRT wrapper, per-platform fallback
        └─ Platform variants (thread caches, huge-page handling)
```

`FMemory` is a static router: every engine allocation goes through it into the active `FMalloc`. Swapping allocators is a platform/build decision, not a per-call one.

## 2. The Allocator Family

- **Binned (default):** small allocations rounded into size classes served from large pooled blocks; near-constant-time alloc/free, some internal fragmentation. Best for the engine's dominant workload (millions of small UObject-adjacent allocations).
- **ANSI:** direct CRT malloc — used for huge allocations and debugging comparisons.
- **FMemStack / frame allocators:** linear bump allocators you reset in one shot; ideal for per-frame or per-operation scratch memory (temp arrays in a loop, parsing, transient geometry). Zero per-item free cost.
- **TInlineAllocator / container strategies:** kill the allocation entirely for small-N cases — the cheapest allocation is the one you never make.

## 3. What Actually Costs

Per-call allocator time is small; the killers are (a) allocation *count* in hot loops, (b) cache misses from pointer-chasing fragmented pools, (c) locks in multi-threaded allocation bursts. Optimization order: eliminate allocations (reserve, inline, memstack) → batch them → only then blame the allocator.

## Verification Checklist

- Memory grows unbounded: `-LLM` (Low Level Memory tracker) tags first, then UObjects vs non-UObject split.
- Hot loop profiling shows malloc: TArray without Reserve, FString temporaries, or per-frame TMap rebuilds.
- Suspected fragmentation: compare peak vs resident after a level transition; binned pools retain by design.

## Architecture

The allocation layer is deliberately boring: one global FMalloc behind one static FMemory router, with specialized bump allocators for scoped work. Unreal's performance culture lives in *avoiding* this layer (inline storage, pooling, frame allocators) far more than in tuning the allocator itself.

## Data Flow

```
NewObject / TArray / FString
        │
        ▼
FMemory::Malloc
        │
        ▼
GMalloc (FMallocBinned3 default)
        ├─ small size → size-class pool (fast path)
        └─ large size → OS-backed allocation
        │
Scoped scratch work → FMemStack bump pointer → one-shot reset
```

## Key Claims

- [extracted] `FMemory` is defined at `Source/Runtime/Core/Public/HAL/UnrealMemory.h` and is the static entry point for all engine allocation.
- [extracted] `FMallocBinned3` is defined at `Source/Runtime/Core/Public/HAL/MallocBinned3.h` and implements the default size-class pooled allocator.
- [extracted] `FMemStack` is defined at `Source/Runtime/Core/Public/Misc/MemStack.h` and provides one-shot-reset linear allocation for scoped scratch work.
- [inferred] Allocation count dominates allocator speed in real profiles, so elimination strategies (reserve, inline, memstack) beat allocator tuning almost every time.

## Edge Cases

- Binned pools retain freed blocks for reuse — process RSS after load is not a leak by itself.
- `FMemStack` memory must not outlive its reset scope; storing pointers into it across frames is a classic latent crash.
- Third-party libraries bypass FMemory entirely unless wrapped — their allocations will not appear in engine stats.

## Boundaries

- The allocation layer stops at raw memory; UObject lifecycle and GC sit above it and are covered in [uobject-reflection-core](uobject-reflection-core.md).
- GPU memory follows a completely separate RHI path and is out of scope here.

## Evidence

- `FMemory` defined at `Source/Runtime/Core/Public/HAL/UnrealMemory.h`
- `FMallocBinned3` defined at `Source/Runtime/Core/Public/HAL/MallocBinned3.h`
- `FMemStackBase` defined at `Source/Runtime/Core/Public/Misc/MemStack.h`
- `FMemStack` defined at `Source/Runtime/Core/Public/Misc/MemStack.h`
- `FGenericPlatformMemoryStats` defined at `Source/Runtime/Core/Public/GenericPlatform/GenericPlatformMemory.h`
