# Galactic Exodus Phase 2 SRS terrain effects

## 1. Scope

This document records the decisions from #1086 for SRS terrain, map-element passability, observation, movement-cost calculation, WARP_POINT placement, and Terrain/Object compatibility.

Generation density and counts remain in #1088. Exact command resolution and turn handling remain in #1089.

## 2. Map sizes

```text
supported map sizes:
  9x9
  11x11

baseline:
  9x9
```

`7x7` is not supported by the Phase 2 SRS model.

## 3. Terrain types

```text
FLOOR
DEBRIS
NEBULA
ASTEROID_FIELD
ASTEROID
GRAVITY_FIELD_VERTICAL
GRAVITY_FIELD_HORIZONTAL
RIFT_DISTORTION
RIFT_BARRIER
```

`WALL` is not used. Impassable cells must have a setting-specific type.

## 4. Object types relevant to movement

```text
STAR
PLANET
STATION
RESOURCE_CACHE
SALVAGE
```

`STATION_STRUCTURE` and `BASE_NODE` are replaced by one impassable, adjacent-interaction `STATION` object.

## 5. Terrain attributes

| id | Japanese name | passable | move multiplier | observation | blocks movement/line travel | can host WARP_POINT |
|---|---|---:|---:|---|---:|---:|
| `FLOOR` | 通常空間 | true | 1 | 5x5 | false | true |
| `DEBRIS` | デブリ帯 | true | 2 | 5x5 | false | false |
| `NEBULA` | 星雲 | true | 2 | 3x3 | false | false |
| `ASTEROID_FIELD` | 小惑星密集域 | true | 3 | 5x5 | false | false |
| `ASTEROID` | 大型小惑星 | false | - | - | true | false |
| `GRAVITY_FIELD_VERTICAL` | 南北重力異常領域 | true | 1 or 2 | 5x5 | false | false |
| `GRAVITY_FIELD_HORIZONTAL` | 東西重力異常領域 | true | 1 or 2 | 5x5 | false | false |
| `RIFT_DISTORTION` | 断層歪曲領域 | true | 2 | 5x5 | false | false |
| `RIFT_BARRIER` | 断層障壁 | false | - | - | true | false |

Only `passable = false` elements block movement and straight-line travel. Passable terrain never blocks a route even when it increases cost or reduces observation.

## 6. Observation

Observation is updated after every successful one-cell step.

```text
1. move to the next cell
2. inspect the destination terrain
3. NEBULA -> observe 3x3; otherwise -> observe 5x5
4. merge the result into the persistent discovered map
5. continue to the next step when movement remains
```

Known cells are cumulative and are not forgotten when entering NEBULA.

A failed move that does not enter a new cell does not trigger destination-cell observation.

## 7. Geometric movement cost

Use integer Euclidean approximation:

```text
ORTHOGONAL_COST = 10
DIAGONAL_COST = 14
```

Route cost is the sum of actual one-cell steps. It is not calculated only from the route endpoints.

```text
step_cost = geometric_step_cost
          * destination_terrain_multiplier
          * gravity_multiplier
```

Examples:

| destination terrain | orthogonal | diagonal |
|---|---:|---:|
| `FLOOR` | 10 | 14 |
| `DEBRIS` | 20 | 28 |
| `NEBULA` | 20 | 28 |
| `ASTEROID_FIELD` | 30 | 42 |
| `RIFT_DISTORTION` | 20 | 28 |

## 8. Gravity fields

### `GRAVITY_FIELD_VERTICAL`

A north-south gravity field. A step whose X coordinate changes has double cost.

```text
if dx != 0:
  gravity_multiplier = 2
else:
  gravity_multiplier = 1
```

### `GRAVITY_FIELD_HORIZONTAL`

An east-west gravity field. A step whose Y coordinate changes has double cost.

```text
if dy != 0:
  gravity_multiplier = 2
else:
  gravity_multiplier = 1
```

Diagonal movement changes both axes, so it is doubled in either gravity-field type.

In a GRAVITY sector, vertical and horizontal cells are selected randomly. Either type alone or both types may occur, but their total count must be at least one. Total placement amount is decided in #1088.

## 9. Impassable elements and STOP_BEFORE

The shared impassable set is:

```text
ASTEROID
RIFT_BARRIER
STAR
PLANET
STATION
```

All use the same behavior:

```text
collision_behavior = STOP_BEFORE
movement_cost_consumed = false
```

Detailed turn consumption, partial-route position, and command semantics are decided in #1089.

## 10. WARP_POINT feature placement

A WARP_POINT represents a gravity-stable location suitable for inter-sector warp.

```text
WARP_POINT may be placed on FLOOR only.
```

Generation invariants:

```text
valid edge midpoint:
  terrain = FLOOR
  feature = WARP_POINT

inner adjacent cell:
  terrain = FLOOR

WARP_POINT cell:
  no object

RIFT blocked edge:
  no WARP_POINT
```

## 11. SectorType x Terrain matrix

Legend:

- `required`: at least one or structurally required
- `optional`: allowed by the sector profile
- `forbidden`: not allowed
- `blocked-edge-required`: required on each blocked edge

| SectorType | FLOOR | DEBRIS | NEBULA | ASTEROID_FIELD | ASTEROID | GRAVITY_VERTICAL | GRAVITY_HORIZONTAL | RIFT_DISTORTION | RIFT_BARRIER |
|---|---|---|---|---|---|---|---|---|---|
| `NORMAL` | required | optional | forbidden | forbidden | forbidden | forbidden | forbidden | forbidden | forbidden |
| `BASE` | required | optional | forbidden | forbidden | forbidden | forbidden | forbidden | forbidden | forbidden |
| `RESOURCE` | required | required | forbidden | optional | optional | forbidden | forbidden | forbidden | forbidden |
| `NEBULA` | required | optional | required | forbidden | forbidden | forbidden | forbidden | forbidden | forbidden |
| `ASTEROID` | required | optional | forbidden | required | required | forbidden | forbidden | forbidden | forbidden |
| `GRAVITY` | required | forbidden | forbidden | forbidden | forbidden | optional-or-required | optional-or-required | forbidden | forbidden |
| `RIFT` | required | optional | forbidden | optional | optional | optional | optional | required | blocked-edge-required |

Additional GRAVITY invariant:

```text
count(GRAVITY_FIELD_VERTICAL)
+ count(GRAVITY_FIELD_HORIZONTAL)
>= 1
```

`RIFT_DISTORTION` is placed randomly among passable cells immediately inside and adjacent to `RIFT_BARRIER`. Placement amount is decided in #1088.

## 12. Terrain x Object matrix

| Terrain | STAR | PLANET | STATION | RESOURCE_CACHE | SALVAGE |
|---|---:|---:|---:|---:|---:|
| `FLOOR` | true | true | true | true | true |
| `DEBRIS` | false | false | false | true | true |
| `NEBULA` | true | true | false | true | true |
| `ASTEROID_FIELD` | false | false | false | true | true |
| `ASTEROID` | false | false | false | false | false |
| `GRAVITY_FIELD_VERTICAL` | true | true | false | true | true |
| `GRAVITY_FIELD_HORIZONTAL` | true | true | false | true | true |
| `RIFT_DISTORTION` | false | false | false | true | true |
| `RIFT_BARRIER` | false | false | false | false | false |

Common object invariants:

```text
WARP_POINT cell:
  no object

impassable terrain:
  no object

one cell:
  at most one object

STAR:
  exactly 1 per SRS map

PLANET:
  multiple per SRS map

STATION:
  exactly 1 in BASE sectors
  FLOOR only
  impassable
  adjacent INTERACT
```

## 13. Deferred decisions

The following are intentionally delegated:

```text
#1088:
  terrain counts and density
  gravity-field total amount
  RIFT_DISTORTION amount/probability
  STAR/PLANET counts and spacing
  deterministic placement retry rules

#1089:
  command schemas
  turn consumption on STOP_BEFORE
  partial-route position
  diagonal corner cutting
  movement-budget exhaustion
  VECTOR_COMMAND rasterization
  DIRECTIONAL_THRUST limits
```
