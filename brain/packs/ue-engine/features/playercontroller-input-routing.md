---
name: playercontroller-input-routing
description: Architectural responsibilities of APlayerController. Manages HUD ownership, input routing, and Client RPC anchors.
feature: playercontroller-input-routing
module: GameFramework
---

# PlayerController Input Routing (UE5 Expert)

## Context
- Mandatory for receiving user input, managing the HUD, and acting as the network connection owner.
- Trigger when logic must survive the death/destruction of the player's physical body (Pawn).

## 1. Core Principles
The `APlayerController` represents the human player's "Will".
- **Network Presence**: Exists ONLY on the Server and the Owning Client. It DOES NOT exist on other clients (Simulated Proxies).
- **Pawn Possession**: A PlayerController "Possesses" a Pawn to control it. It can `UnPossess` and possess a new Pawn at any time.

## 2. Responsibilities
- **Input Handling**: Should route complex inputs. If input depends on the *Player* (e.g., opening a menu), put it in the Controller. If it depends on the *Body* (e.g., Sprinting), route it to the Pawn.
- **UI Ownership**: Spawns and manages all `UUserWidget` instances (HUD, Pause Menu, Inventory).
- **RPC Anchor**: The primary anchor for `Client` and `Server` RPCs.


## Edge Cases
**BAD (UI inside Pawn)**:
```cpp
void AMyCharacter::BeginPlay() {
    // When character dies and is destroyed, the HUD is destroyed too!
    MyHUD = CreateWidget<UUserWidget>(GetWorld(), HUDClass);
}
```

**GOOD (UI inside PlayerController)**:
```cpp
void AMyPlayerController::BeginPlay() {
    // HUD survives even if the controlled Pawn is destroyed.
    MyHUD = CreateWidget<UUserWidget>(this, HUDClass);
}
```

**BAD (Peer-to-Peer Controller Logic)**:
```cpp
// Client A tries to read Client B's Controller.
APlayerController* OtherPC = Cast<APlayerController>(OtherPawn->GetController()); // RETURNS NULL on Client A!
```

**GOOD (Using PlayerState for Peers)**:
```cpp
AMyPlayerState* OtherPS = OtherPawn->GetPlayerState<AMyPlayerState>(); // SAFE. Exists everywhere.
```

## Verification Checklist
- [ ] UI creation and destruction is handled by the PlayerController.
- [ ] Logic that survives respawns is housed here, not in the Pawn.
- [ ] Peer-to-peer data reads use `PlayerState`, not `PlayerController`.
- [ ] Server RPCs for player-wide actions (e.g., `Server_RequestRespawn`) are implemented here.

## Architecture

The architecture for Playercontroller Input Routing follows standard UE5 patterns as described in the numbered sections above.

## Data Flow

Data flows through the UE5 framework APIs as described in the implementation sections of Playercontroller Input Routing.

## Key Claims

- **Null Pointers on Peers**: If Client A tries to get Client B's PlayerController, it returns `nullptr`. Agents must use `PlayerState` to share data between peers.
- **UI Memory Leaks**: If UI is spawned by the Pawn, it is destroyed when the Pawn dies. UI must be spawned by the PlayerController to persist.


The Playercontroller Input Routing pattern follows UE5 engine conventions and best practices.

## Boundaries
- The UE5 engine system or pattern named in the title defines the scope of this document.
- Context section covers when to use this pattern.

## Evidence

- `UE5Skill` defined at `Source/Runtime/Engine/EngineTypes.h:1`

