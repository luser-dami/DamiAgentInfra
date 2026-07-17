---
domain: Networking
tags: [networking, replication, prediction]
source: draft
---

# Networking

Lyra's multiplayer is built on Unreal's actor replication, the Gameplay
Ability System's prediction, and the network push model. This document is an
early draft: several sections are still placeholders and should be treated as
incomplete until filled in.

## Overview

Lyra networking rests on three pillars: standard Unreal Actor replication for
world and pawn state, Gameplay Ability System prediction for responsive local
ability activation, and the push-model replication to reduce per-frame
comparison cost. Authority stays server-side; clients predict and reconcile.

## Replication Graph

TODO

## Prediction

Placeholder.

## Open Questions
