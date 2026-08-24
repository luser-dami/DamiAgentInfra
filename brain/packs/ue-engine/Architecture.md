---
architecture: Unreal Engine 5
tags: [ue5, unreal, engine, overview, cpp]
source: manual
---

# Unreal Engine 5 C++ Expert Skills

A curated library of Unreal Engine 5 C++ skill documents covering core engine
systems, performance optimization, rendering, networking, Gameplay Ability
System, editor customization, and more.

## Context

- **Scope:** engine-level knowledge base (UE 5.0–5.6)
- **Skill categories:** Core C++ Patterns, GAS, Rendering, Networking,
  Editor Tools, AI, UI/Slate, Performance, Animation, Audio
- **Document format:** YAML frontmatter + structured sections
- **Source:** https://github.com/mrSutivu/Unreal-Engine-5-C-Expert-Skills

## Architecture

Each skill document covers one engine concept with implementation patterns,
best practices, and common pitfalls. Documents are organized as module-tier
knowledge units scoped to Unreal Engine 5.

## Data Flow

Readers navigate from this overview into specific module documents by topic.
Each module doc is self-contained with Context, implementation sections,
Key Claims, Edge Cases, Boundaries, and Evidence.


Document index:

**domains/ai/**
- [AI Controller & Possession](domains/ai/ai-controller-possession.md)
- [Blackboard Data Access](domains/ai/blackboard-data-access.md)
- [Behavior Tree Services & Decorators](domains/ai/bt-service-decorator.md)
- [Behavior Tree Tasks in C++](domains/ai/bt-task-cplusplus.md)
- [PCG Custom C++ Nodes](domains/ai/pcg-cpp-nodes.md)
- [State Trees AI](domains/ai/state-trees-ai.md)

**domains/animation/**
- [AnimMontage Execution in C++](domains/animation/anim-montage-cpp.md)
- [AnimNotifies in C++](domains/animation/anim-notifies-cpp.md)
- [Thread-Safe AnimInstance](domains/animation/thread-safe-animinstance.md)

**domains/editor/**
- [Custom Asset Types](domains/editor/custom-asset-types.md)
- [Custom Blueprint Nodes (UK2Node)](domains/editor/custom-blueprint-nodes-uk2node.md)
- [Custom Details Panel](domains/editor/custom-details-panel.md)
- [Customizing the Outliner & Content Browser](domains/editor/customizing-outliner-content-browser.md)
- [Editor Only Module](domains/editor/editor-only-module.md)
- [Responding to Editor Selection Changes](domains/editor/editor-selection-changes-ui.md)
- [EditorUtilityBlueprint](domains/editor/editorutilityblueprint.md)
- [EditorUtilityWidget](domains/editor/editorutilitywidget.md)
- [IAssetActionUtility](domains/editor/iassetactionutility.md)

**domains/gas/**
- [GAS: Ability Replication & Instancing Policies](domains/gas/gas-ability-replication-instancing.md)
- [GAS: Custom Ability Tasks](domains/gas/gas-ability-tasks.md)
- [GAS: ASC Loose Gameplay Tags](domains/gas/gas-asc-loose-gameplay-tags.md)
- [GAS: Execution Calculations](domains/gas/gas-execution-calculations.md)
- [GAS: Target Data Handling](domains/gas/gas-target-data.md)
- [GAS: UAttributeSet Architecture](domains/gas/gas-uattributeset.md)
- [GAS: UGameplayAbility](domains/gas/gas-ugameplayability.md)
- [GAS: UGameplayEffect](domains/gas/gas-ugameplayeffect.md)

**domains/networking/**
- [Fast TArray Replication](domains/networking/fast-tarray-replication.md)
- [Network Authority & Role](domains/networking/network-authority-role.md)
- [Property Replication & DOREPLIFETIME](domains/networking/property-replication-doreplifetime.md)
- [Push Model Replication](domains/networking/push-model-replication.md)
- [RepNotify Architecture](domains/networking/repnotify-architecture.md)
- [Remote Procedure Calls](domains/networking/rpc-server-client-multicast.md)
- [Seamless Travel Persistence](domains/networking/seamless-travel-persistence.md)

**domains/performance/**
- [Casting Cost & Type Checking](domains/performance/performance-casting-type-checks.md)
- [Class Default Object (CDO) Read-Only](domains/performance/performance-cdo-read-only.md)
- [Math & String Optimization](domains/performance/performance-math-simd-strings.md)
- [Object Pooling](domains/performance/performance-object-pooling.md)
- [Struct Padding & Cache Locality](domains/performance/performance-struct-padding-cache.md)
- [Tick Eradication & Event-Driven Logic](domains/performance/performance-tick-event-driven.md)

**domains/rendering/**
- [Compute Shaders in RDG](domains/rendering/compute-shaders-rdg.md)
- [Custom ACES Tonemapper](domains/rendering/custom-aces-tonemapper.md)
- [Declare Global Shader](domains/rendering/declare-global-shader.md)
- [HLSL Custom Expressions](domains/rendering/hlsl-custom-expressions.md)
- [Nanite Skeletal Meshes & MegaLights (UE5.5 Expert)](domains/rendering/nanite-skeletal-meshes-megalights.md)
- [Post-Process Material Logic](domains/rendering/post-process-material-logic.md)
- [Custom Post-Process Pass via RDG](domains/rendering/rdg-post-process-pass.md)
- [Render Dependency Graph (RDG)](domains/rendering/render-dependency-graph-rdg.md)
- [SceneTexture and G-Buffer Access in HLSL](domains/rendering/scene-texture-hlsl-access.md)
- [Shader Compilation & Debugging](domains/rendering/shader-debugging-compilation.md)
- [Shader Parameter Structs](domains/rendering/shader-parameter-structs.md)
- [Shader Permutation Reduction](domains/rendering/shader-permutation-reduction.md)
- [Tonemapper Pipeline Architecture](domains/rendering/tonemapper-pipeline-architecture.md)
- [USF Shader File Management](domains/rendering/usf-shader-file-management.md)
- [Vertex Shader Manipulation (WPO)](domains/rendering/vertex-shader-manipulation-wpo.md)

**features/**
- [ActorComponent Modularity](features/actor-component-modularity.md)
- [Automation Testing Framework](features/automation-testing-framework.md)
- [BindWidget](features/bindwidget.md)
- [BlueprintImplementableEvent](features/blueprint-implementable-event.md)
- [BlueprintNativeEvent Implementation](features/blueprint-native-event.md)
- [Class Prefixes](features/class-prefixes.md)
- [Collision Profiles & Channels](features/collision-profiles-channels.md)
- [C++20 Lambda Captures](features/cpp20-lambda-captures.md)
- [Custom Character Movement](features/custom-character-movement.md)
- [DataTables & FTableRowBase](features/datatables-ftablerowbase.md)
- [Defensive Programming](features/defensive-programming.md)
- [Documentation and Change-logging](features/documentation-and-change-logging.md)
- [Dynamic Material Instances](features/dynamic-material-instances.md)
- [Enhanced Input System Mapping](features/enhanced-input-system.md)
- [Fab Marketplace Submission Requirements](features/fab-marketplace-submission-requirements.md)
- [FArchive Binary Serialization](features/farchive-binary-serialization.md)
- [FCollisionQueryParams Configuration](features/fcollisionqueryparams.md)
- [FInstancedStruct](features/finstancedstruct.md)
- [Forward Declaration](features/forward-declaration.md)
- [Framework Reference Routing](features/framework-reference-routing.md)
- [Game Features & Modularity](features/game-features-modular.md)
- [GameInstance Cross-Level State](features/gameinstance-cross-level-state.md)
- [GameMode Server Authority](features/gamemode-server-authority.md)
- [GameplayTag Integration](features/gameplay-tag-integration.md)
- [GameState Global Replication](features/gamestate-global-replication.md)
- [Garbage Collection Optimization](features/garbage-collection-optimization.md)
- [Hot Reload vs. Live Coding](features/hot-reload-vs-live-coding.md)
- [Interface Implementation](features/interface-implementation.md)
- [Low-Level Memory Allocators](features/low-level-memory-fmemstack.md)
- [Mass Entity ECS](features/mass-entity-ecs.md)
- [Material Parameter Collections](features/material-parameter-collections.md)
- [MetaSounds C++ Integration](features/metasound-cpp-parameters.md)
- [Multi-Platform Support Verification](features/multi-platform-support-verification.md)
- [Multicast Dynamic Delegates](features/multicast-dynamic-delegates.md)
- [Native SListView / STileView Implementation](features/native-slistview-stileview.md)
- [Niagara C++ Integration](features/niagara-cpp-integration.md)
- [Overlap and Hit Events](features/overlap-hit-events.md)
- [Pawn vs. Character & Possession](features/pawn-vs-character-possession.md)
- [Physics Materials in C++](features/physics-materials-cpp.md)
- [Player Camera Manager Control](features/player-camera-manager-control.md)
- [PlayerController Input Routing](features/playercontroller-input-routing.md)
- [PlayerState Data Persistence](features/playerstate-data-persistence.md)
- [Plugin Module Configuration](features/plugin-module-configuration.md)
- [Plugin Resources Management](features/plugin-resources-management.md)
- [Primary Data Assets](features/primary-data-assets.md)
- [Programming Subsystems Architecture](features/programming-subsystems-architecture.md)
- [Raycasts & Line Traces](features/raycast-line-trace.md)
- [SCompoundWidget](features/scompoundwidget.md)
- [Slate Style Sets](features/slate-style-sets.md)
- [Slate Syntax](features/slate-syntax.md)
- [Smart Reference Counting](features/smart-reference-counting.md)
- [Static Blueprint Libraries](features/static-blueprint-libraries.md)
- [TObjectPtr Migration](features/tobjectptr-migration.md)
- [Unreal Build Tool (UBT) Integration](features/ubt-integration.md)
- [UE5 Smart Pointers](features/ue5-smart-pointers.md)
- [UE5Coro & Coroutines](features/ue5coro-coroutines.md)
- [UE_LOG Formatting and Verbosity](features/uelog-formatting.md)
- [UI Performance & Invalidation Boxes](features/ui-performance-invalidation-boxes.md)
- [TArray, TMap, TSet Selection Logic](features/unreal-containers-tarray-tmap-tset.md)
- [UObject Metadata](features/uobject-metadata.md)
- [UPROPERTY Specifiers](features/uproperty-specifiers.md)
- [USaveGame Serialization](features/usavegame-serialization.md)
- [UserWidget Lifecycle Management](features/userwidget-lifecycle-management.md)
- [UWidgetBlueprintLibrary](features/uwidgetblueprintlibrary.md)
- [Visual Logger (UE_VLOG)](features/visual-logger-vlog.md)
- [World Partition & Data Layers](features/world-partition-data-layers.md)

**modules/**
- [AbilitySystemComponent Internals](modules/ability-system-component.md)
- [Actor Lifecycle](modules/actor-lifecycle.md)
- [Animation Evaluation Pipeline](modules/animation-evaluation-pipeline.md)
- [Async Loading with TSoftObjectPtr](modules/async-loading-tsoftobjectptr.md)
- [Async Task Graph](modules/async-task-graph.md)
- [CharacterMovementComponent Internals](modules/character-movement-component.md)
- [Enhanced Input: Triggers & ETriggerEvent](modules/enhanced-input-triggers.md)
- [GameplayTags Manager Internals](modules/gameplaytags-manager.md)
- [Garbage Collection Internals](modules/garbage-collection.md)
- [Memory Allocation Internals](modules/memory-allocation.md)
- [Navigation & Pathfinding Internals](modules/navigation-pathfinding.md)
- [NetDriver, Channels & Replication](modules/netdriver-channel-replication.md)
- [Platform Abstraction / HAL](modules/platform-abstraction-hal.md)
- [Rendering Frame Pipeline](modules/rendering-frame-pipeline.md)
- [Sequencer & Cinematics](modules/sequencer-cinematics-cpp.md)
- [Slate & UMG Architecture](modules/slate-umg-architecture.md)
- [FString vs. FName vs. FText](modules/string-handling-fstring-fname-ftext.md)
- [TSubclassOf for Designer Interop](modules/tsubclassof-designer-interop.md)
- [UObject & Reflection Core](modules/uobject-reflection-core.md)

**references/**
- [Actor Network Lifecycle (UE5 Expert Reference)](references/actor-network-lifecycle.md)
- [Async & Multi-Threading Templates (UE5 Expert Reference)](references/async-multithreading-tasks.md)
- [Deep Editor Customization Templates (UE5 Expert Reference)](references/deep-editor-customization.md)
- [GAS Attribute & Execution Template (UE5 Expert Reference)](references/gas-attribute-execution.md)
- [Multiplayer Seamless Travel Template (UE5 Reference)](references/multiplayer-seamless-travel-code.md)
- [RDG Pipeline Template (UE5 Expert Reference)](references/rdg-pipeline-template.md)
- [Slate UI Macro Lexicon (UE5 Reference)](references/slate-ui-macro-lexicon.md)
- [Native UE5 Memory Patterns (Expert Reference)](references/ue5-native-memory-patterns.md)

## Key Claims

- The UE5 Expert Skills library is an engine-level shared knowledge base curated from https://github.com/mrSutivu/Unreal-Engine-5-C-Expert-Skills
- Each skill document covers a single engine concept with C++ implementation examples, Context, Edge Cases, and Boundaries.

## Edge Cases

- Some documents reference UE5 alpha/preview features (e.g., Nanite Skeletal Meshes) that may change between engine versions.
- Version-specific syntax is noted in the document title or context section.

## Boundaries

- Does not cover Blueprint-only workflows.
- Does not cover project-specific game logic.
- Does not cover UE4 legacy systems not present in UE5.

## Evidence

- `BrainEngine` packs defined at `BrainEngine/packs/ue-engine/Architecture.md:1`
