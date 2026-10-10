# Realistic vegetation (design)

Phase 0 of the tree project: what OMSI 2 trees are, how they travel through openOMSI today,
what the stock maps contain, and the design that replaces their look in `enhanced`. This
document decides; the code follows it in the PRs of section 6. File references are
`path:line` as of the commit this was written against.

## 1. Goal and hard rules

Replace the look of trees on every OMSI 2 map, at load and draw time, without touching map
files or the installation. The rules:

* No writes to OMSI files (installation, maps, scenery); everything generated lives in
  `~/.openomsi/cache` or in memory.
* No original assets are copied. New assets are procedural, or CC0 with the licence stored
  next to them (decision (a), section 8).
* `vanilla` and `vanilla_plus` stay unchanged pixel for pixel: `scripts/golden.sh` vanilla
  scenes differ by 0 px. The new trees exist only in `enhanced` / `enhanced_plus` and can
  be switched off.
* Gameplay is unchanged: collisions, AI, scripts and LAN play are untouched. We replace only
  what is drawn.
* The look is deterministic: the seed comes from the map object id, the tile and the
  position, so every machine (LAN included) draws the same tree.
* The layering of `docs/ARCHITECTURE.md` holds: the generator crate has no GPU and no
  clock; the renderer does not read game types (`.sco`, map records).
* Debug flags go through `omsi_cfg::flags`; new files stay under the size budget
  (`scripts/size-budget.sh`: file <= 1500 lines, function <= 300).

## 2. Findings (reconnaissance)

### 2.1 How OMSI 2 represents trees

**The `[tree]` section.** A `.sco` file declares `[tree]` followed by `texture, h_a, h_b,
ratio_a, ratio_b` (parsed at `crates/omsi-scenery/src/sco.rs:387-390`, stored in
`SceneryObject.tree`, `sco.rs:166`). The order of the two values of a pair is not
guaranteed: `Birke01.sco` has `18 13`, `Trees_MC/tree_large_01.sco` has `20 30` and
`1 1.3`. openOMSI reads the pairs as (min, max) (`place.rs:724`; unverified against
`Omsi.exe`), which is harmless because only their midpoint is used. The `[mesh]` of such an
object is `treehelper.x` marked `[onlyeditor]`: the editor shows a helper, the game a
picture.

```
[friendlyname] Birke 01
[nocollision]
[tree]
Birke01.dds
18
13
0.62
0.88
[mesh]
treehelper.x
[onlyeditor]
```

**The record in the map.** A tree record carries three labels after the common fields:
texture, height and ratio (`docs/FORMATS.md:233-235`, keyword list `:405`). The map author
picks a texture per instance: this record places `tree_medium_04.sco` with `kastanie01.tga`
at a height of 14.352 m and a ratio of 1.167 (slashes stand for the line breaks of the
file):

```
Object Nr. 22 / [object] / 0 / Sceneryobjects\Trees_MC\tree_medium_04.sco / 5716 /
295.81 / 213.63 / 0 / 257.76 / 0 / 0 / 4 / kastanie01.tga / 14.352 / 1.167 / (empty)
```

So the texture of a tree is a property of the instance, not of the `.sco`; classification
(4.2) works on the pair (sco, effective texture).

**How openOMSI draws it.** One crossed-card mesh (`tree_quad_mesh`,
`crates/omsi-app/src/scene/terrain_paint.rs:232-238`) per tree, with an alpha test
(`place_step.rs:554-618`, `AlphaMode::Test`). The scale is (width, width, height) with
width = height x ratio (see 4.2).

**Plant models.** The other kind of plant is an ordinary object with alpha materials:
`Ruede/trees_chestnut01.sco` has a `.x` mesh, `Trees_MC/shrubbery_01_05.sco` an `.o3d` with
`[matl_alpha] 1` and `[complexity] 2`. They keep their collision mesh and object logic.
They are recognised by `vegetation_give_of` (`terrain_paint.rs:155-183`: `GROUP_WORDS`,
`NAME_WORDS`; tests `scene/tests.rs:429-436`).

**Backdrops.** Forest pictures on wide cards (`Ruede/forest_backdrop01`) stand far from
the road. They are excluded from this project.

**Small things under `[tree]`.** The section is also used for flowers (`Blumen_rosa_1`,
1 m) and ground cover (`Bodengestruepp03`, 1.3-4 m). These must never become trees: the
classifier returns `None` for them, by ground-cover words or by height (4.2).

**Seasons.** Texture folders `Spring`, `Fall`, `Winter`, `WinterSnow`, `WinterSnowfall`
and `Night` (lowercase `fall/spring/winter` in `Trees_MC`); the lookup chain is
`WinterSnowfall -> WinterSnow -> Winter -> Fall` (`omsi-texture/src/lib.rs:238-262`,
`docs/FORMATS.md:560-567`). Each plant gets its own season phase from the mix by plant
key (`season_looks.rs:41-46`, `season_mix.rs:14-39`, `tree_look_texture` `:62-83`). There is
no `_LOW` variant for trees and no far object specific to them.

**Existing wind.** `tree_sway` (`shader.wgsl:555-636`; cantilever bend about the pivot
`material.sway.y`, gusts from `camera.tree_wind`, phase from the foot position `:593`),
switched by the setting `windy_trees` (`settings.rs:196`, default on, off for the Low
preset). Tree cards use pivot 0.28, shrubs 0.03 (`terrain_paint.rs:222-230`).

**No coverage-preserving mips.** The CPU downsample `bc::downsample` (`bc.rs:657`) is a
linear box filter, the GPU one is `mip.wgsl`; the alpha-test threshold `ALPHA_REF`
(128.5/255, `shader.wgsl:335`) therefore sees less and less coverage on the higher mips,
and trees thin out with distance.

### 2.2 Data path

| Stage | Where | What happens |
|---|---|---|
| `.sco` parse | `omsi-scenery/src/sco.rs:387-390` | `[tree]` into `SceneryObject.tree` |
| Map tile parse | `omsi-map/src/tile.rs:7-31, 294-363` | `MapObject { file, id, pos, rot, extra }`; `extra` = the labels |
| Stage | `omsi-app/src/scene/stage.rs:787-873` | `stage_objects`; `StagedObject.key = o.id` (`:872`); `placed_type` `:973-1021`; ground `:1093-1156` |
| `place_tile` | `scene/place.rs:226` | trees diverted into `trees` at `:321-326` (hook 1, before `tree_of`) |
| `tree_of` | `place.rs:722-753` | texture, height, width (height x ratio) from the labels or the midpoint of the `.sco` ranges (fallback 10 m / 1); no randomness |
| `place_trees` | `scene/place_step.rs:554-618` | one `tree_quad_mesh` instance per tree, `Scale(width, width, height)`, `MaterialExtra { tree, sway }` (hook 2) |
| Mesh upload | `upload.rs:260` | the card mesh goes to the renderer |
| Renderer `Instance` | `omsi-render/src/lib.rs:1196-1295` | `add_instance` `:4759`; `set_lod_range` `:5027` |
| `cull_view` | `passes/cull.rs:74` | frustum / distance cull per view |
| Batch | `passes/batch.rs:136, 254, 474-601` | `opaque_items` (alpha test counts as opaque), `batch_draws`, `sort_blended_draws` |
| Draw | `PIPE_ALPHA_TEST` `lib.rs:8937-8943` | alpha-tested pipeline, A2C with MSAA |

The object id never reaches the renderer for trees today; the vegetation path will carry
a seed instead (4.4). The LOD of ordinary models is `omsi-model/src/lib.rs:194-198,
548-566`, `cull.rs:201-202` and `place_step/parts.rs:135-200`.

### 2.3 The enhanced pipeline today

* **Pass order** (`render_inner`, `lib.rs:7036`): sun shadows, lamp shadows, depth
  prepass + SSAO, ray tracing, sky cube, reflection probe, main pass, RT / puddle
  reflections, fog lamps, rain film, classic present, post, upscale. Three encoders (shadow,
  prepass, picture) are finished on helper threads (`passes/submit.rs:24-54`). The main
  pass (`passes/main_pass.rs:235`) records bundles on threads (`record_main_bundles`,
  `run_parts` `lib.rs:9104`).
* **Shadows:** three cascades at 32 m / 140 m / 700 m (`lib.rs:2534-2538`) plus four lamp
  tiles (`:498`); resolution 1024 / 2048 / 4096, default 2048 (`:2261-2270`); tent PCF of
  2 texels with 4 gathers (`shader.wgsl:975-985`); alpha-tested casters in `fs_shadow_test`
  (`:872-892`); bias `:942-952`, pipeline depth bias 4 / slope 3. The near cascade is drawn
  every second frame, the far one more rarely (`passes/shadow_plan.rs`).
* **Anti-aliasing:** MSAA 4x by default (`lib.rs:2259`), 2x on weak GPUs
  (`gpu_memory.rs:141-180`); alpha-to-coverage for `PIPE_ALPHA_TEST` whenever samples > 1
  (`pipelines/scene.rs:99,131,147-150`; `enhanced.wgsl:582-590`); FXAA only without MSAA
  (`passes/post.rs`). There is no TAA and there are no motion vectors.
* **Light:** CPU Hillaire 2020 atmosphere with an SH order-2 ambient (`atmosphere.rs`), sky
  cube and reflection probe (`pipelines/enhanced.rs:112-266`). Night lights sit in a 2D
  grid of 64x64 cells of 25 m with 32 lights per cell (`lib.rs:495-530`,
  `enhanced.wgsl:223-260`).
* **Mirrors** are separate cameras (`camera_util.rs:341-369`, `Instance::mirror_only`
  `lib.rs:1272`) with a raised minimum object size (`:381-384`), no shadows (`:385`) and
  plain shading (`setup.rs:101-111`).
* **Snow** is applied by the up-facing normal (`enhanced.wgsl:885-892`); there is no tree
  specific test.
* **Per-draw instance parameters** are two `vec4` (`[alpha, visible, uv.x, uv.y]`,
  `[slot_light, slot_night, interior, surface code]`, `lib.rs:5852-5874`) and have no free
  slot: a per-instance tint needs a third `vec4` or a buffer of its own.
* **Vertex format:** position, normal, uv, 32 B (`lib.rs:34-39`).
* **Profiling:** `OMSI_PROFILE` stages (`StageClock::stage`, `passes/mod.rs:165-185`) and
  GPU pass timers with `OMSI_GPU_TIMERS` (`lib.rs:8660-8720`).

### 2.4 Backends

The renderer is a wgpu 29 fork (`Cargo.toml:92-105`, features metal, vulkan, dx12, gles,
wgsl, noop). Backend order (`startup.rs:304-332`): macOS Metal; Windows DX12, Vulkan, GL,
ANGLE; Linux and Android Vulkan, then GLES.

| Capability | State |
|---|---|
| Compute | only in `rt.rs:727-871` (ray queries), texture means and the probe; never on GLES (needs ES 3.1) |
| Indirect draw | not used anywhere; needs ES 3.1; no `MULTI_DRAW` on GLES |
| Vertex storage buffers | maybe absent on GLES: `device_selection.rs:4-27` classifies `Storage` / `VertexTextures` / `NoStorage` (`OMSI_GPU_ARRAYS`) |
| BC textures | often missing on GLES (needs s3tc_srgb + rgtc + bptc); the repo then decodes to RGBA (`lib.rs:2733-2737, 2818-2824`); ETC2 / ASTC unused |
| Alpha-to-coverage | works on GLES |
| MSAA | steps down `[wanted, 8, 4, 2, 1]` (`lib.rs:2798`) |
| Limits | `lib.rs:2653-2677`; `OMSI_GPU_LIMITS=downlevel` for tests |

Consequence: the baseline path of the design uses no compute and no indirect draws, only
vertex / index buffers, instancing and render passes. Faster paths are optional extras.

### 2.5 Related issues

#1720 (foliage aliasing in Enhanced+), #1084 (winter trees: pale ghosts with dark holes),
#1666 (enhanced lighting on trees looks unrealistic); closed: #105 (foliage alpha
coverage), #1480 (tree card edge line), #1677 (far forest backdrop visibility). #101 / #102
(mip colour kept under alpha 0) are why transparent texels keep their colour. Open and
unrelated: #947, #1063.

## 3. Inventory

`cargo run --locked --release -p omsi-check -- "<OMSI 2>" --only trees` walks every map
and counts the placed trees (`tools/omsi-check/src/main.rs`; `--trees-out` writes one CSV row per
(map, object type)). Result for the stock install:

| Map | Tiles | Objects | `[tree]` instances (types) | Plant-model instances (types) | Backdrops |
|---|---:|---:|---:|---:|---:|
| Grundorf | 19 | 1 117 | 428 (18) | 134 (8) | 37 |
| Berlin-Spandau | 369 | 30 897 | 8 390 (33) | 5 766 (20) | 317 |
| Thüringer Wald | 191 | 299 480 | 175 821 (64) | 104 573 (63) | 714 |
| Krakow | 9 | 10 | 0 | 0 | 0 |

All instances resolved (0 unresolved on every map). The maps `Name` and `NewMap` are empty
test maps.

Top 10 types, Grundorf (instances; `h` = height range of the `[tree]` section):

| Instances | Kind | Object | h (m) | Texture |
|---:|---|---|---|---|
| 54 | tree | `Trees_MC\tree_medium_04` | 12.1-17.8 | `kastanie01.tga` |
| 47 | tree | `Trees_MC\tree_medium_02` | 12.0-17.6 | `tree_medium_02.tga` |
| 45 | tree | `Trees_MC\tree_medium_06` | 12.1-17.9 | `tree_medium_06.tga` |
| 40 | plant | `Ruede\trees_chestnut02` | - | - |
| 38 | plant | `Ruede\trees_chestnut03` | - | - |
| 34 | tree | `Trees_MC\tree_medium_09` | 12.6-17.9 | `tree_medium_09.tga` |
| 33 | tree | `Trees_MC\tree_medium_05` | 10.0-14.7 | `tree_medium_05.tga` |
| 32 | tree | `Trees_MC\tree_medium_08` | 12.1-17.8 | `tree_medium_08.tga` |
| 31 | plant | `Ruede\trees_chestnut01` | - | - |
| 29 | tree | `Trees_MC\tree_medium_01` | 12.1-17.9 | `tree_medium_01.tga` |

Top 10 types, Berlin-Spandau:

| Instances | Kind | Object | h (m) | Texture |
|---:|---|---|---|---|
| 1 352 | plant | `Trees_MC\shrubbery_03_20` | - | - |
| 1 285 | plant | `Trees_MC\shrubbery_04_20` | - | - |
| 1 077 | tree | `Trees_MC\tree_medium_04` | 9.0-20.0 | `kastanie01.tga` |
| 1 001 | plant | `Trees_MC\shrubbery_01_20` | - | - |
| 996 | tree | `Trees_MC\tree_medium_02` | 8.0-18.0 | `tree_medium_02.tga` |
| 623 | tree | `Trees_MC\tree_medium_01` | 10.0-18.0 | `tree_medium_01.tga` |
| 568 | tree | `Trees_MC\tree_medium_05` | 10.0-15.0 | `tree_medium_05.tga` |
| 534 | tree | `Trees_MC\tree_medium_03` | 12.0-18.0 | `tree_medium_03.tga` |
| 511 | plant | `Ruede\trees_chestnut02` | - | - |
| 503 | tree | `Trees_MC\tree_medium_07` | 12.0-18.0 | `tree_medium_07.tga` |

Key takeaways:

* `[tree]` dominates on the stock maps: Spandau 8 390 `[tree]` instances against 5 766
  plant models, Grundorf 428 against 134. The card is therefore the main target, and the
  plant models (shrubs, chestnuts) the second one.
* Many Spandau "trees" are small: `tree_small_*`, avenue trees, shrubs. Height and
  archetype must come from the instance, not from the file name.
* Thüringer Wald has about 176 000 `[tree]` plus about 105 000 plant-model instances, and
  its top types are ground cover and bushes (`Bodengestruepp*`, `Ebereschengestruepp`,
  `Gras`, 1.3-27 m) as much as trees. The design must scale to 10^5 instances per map,
  with tens of thousands within view distance: **impostors and cluster culling are
  mandatory, not optional.**
* The stock texture formats vary: `Trees_MC` TGA32 up to 1024x1024, TH Wald `Trees_MC`
  DXT5 up to 2048x2048 without mips, Tristan98 DXT5 with mips, Perotinus 2048x2048
  without mips. Texture-only mode (4.8) must cope with all of them.

## 4. Architecture

### 4.1 Crates

* **NEW `crates/omsi-vegetation`** (layer 1: pure data, no GPU, no clock). Modules:
  `classify` (signals -> `Archetype` + `confidence: f32`), `overrides` (parser of
  `Vegetation/*.cfg`), `seed`, `gen` (skeleton, meshes, LODs), `cache` (keys and
  (de)serialisation of meshes), `presets`, `tex` (procedural bark and leaf textures as RGBA
  buffers). Depends on `omsi-cfg`, `omsi-scenery`, `glam`, `rayon`; nothing else new.
* **`omsi-texture`:** coverage-preserving mip generation, a new function next to
  `bc::downsample` (`bc.rs:657`). BC encoding of the generated textures uses the existing
  `bc::encode` (`bc.rs:544`).
* **`omsi-render`:** new module `vegetation/` (pipelines, instance buffers, culling, LOD
  fade, impostor bake and draw, wind). It takes plain data (meshes, textures, an instance
  list) and knows nothing of `.sco`.
* **`omsi-app`:** the hook at `scene/place.rs:321` and `place_step.rs:554` (`place_trees`).
  In `enhanced` with Trees != Original, a classified tree becomes a vegetation instance
  instead of a card instance. Collisions and objects are untouched: trees have
  `[nocollision]`; plant models keep their collision mesh and object logic, and only their
  visual instance is hidden.

### 4.2 Classification (Phase 1)

Per pair (sco, effective texture) the classifier sums weighted signals:

| Signal | Weight |
|---|---:|
| `[tree]` section present | +0.5 |
| species word in texture, file name or groups (Birke, Eiche, Buche, Linde, Ahorn, Kastanie, Pappel, Kiefer, Fichte, Tanne, Weide, and the English names) | +0.3 |
| plant group words (`GROUP_WORDS`, `NAME_WORDS`) | +0.2 |
| height >= 3 m (below 2 m: shrub or flower) | +0.1 |
| ground-cover words in file, texture or groups (gras, gestruepp, gestrüpp, blume, farn, unkraut, flower, grass, fern, weed) | `None` (texture-only), regardless of height |
| alpha share of the materials (plant models) | tie-breaker |

Archetypes: `BroadleafWide`, `BroadleafNarrow` (columnar: Pappel), `ConiferConical`
(Fichte, Tanne), `Pine` (Kiefer), `Willow` (Weide), `Birch`, `Shrub`, `Hedge`, and `None`
(flowers, unknown things). Confidence below 0.6 means texture-only mode for that tree
(4.8). An unknown `[tree]` without a species word becomes `BroadleafWide` with confidence
0.6 (the most common case); a height below 2 m, or any ground-cover word, gives `None`.

**Target dimensions.** For `[tree]` the height is the label `o.extra[1]` if positive, else
the midpoint of the `.sco` range (fallback 10 m); the ratio is `o.extra[2]` if positive,
else the midpoint of the ratio range (fallback 1); and **width = height x ratio**
(`tree_of`, `place.rs:722-753`; the comment there cites `Omsi.exe` 0x77e6b0 / 0x774444).
The card is scaled by `(width, width, height)` (`place_step.rs:554-618`), so the crown
of the generated tree must fit a box of `width x width x height`; the ratio multiplies,
it does not divide. For plant models the target is the bounding box of the mesh.

### 4.3 Overrides

Every `*.cfg` in the folder `Vegetation` of the content folder, read in file-name order, in
the OMSI-style keyword-block format parsed with `omsi-cfg` (no new external crate). Example:

```
[rule]
sco
Sceneryobjects\Trees_MC\tree_medium_0*.sco
broadleaf_wide
linden
```

Four lines after `[rule]`: target (`sco` or `texture`), glob pattern (`*`, `?`, case and slash
direction ignored), archetype name (`none` keeps the original look), species or `-`. The first
match wins and counts as certain. Parsed by `omsi_vegetation::overrides`; documented in
`docs/MODDING.md`.

### 4.4 Determinism

```
seed = splitmix64(object_id ^ tile_x << 40 ^ tile_y << 20 ^ quantise(pos, 0.1 m))
```

reusing `season_mix::unit_hash` (`season_mix.rs:31-39`, splitmix64). From the seed come the
variant index, the yaw, the scale jitter (+-6 %, applied around the target height so the
author's composition stays), the tint (hue +-4 degrees, value +-8 %) and the crown health.
No clock, no thread order and no hash-map iteration order enters it: the same seed on
every machine, LAN included.

### 4.5 Generator (Phase 2)

The skeleton follows the Weber-Penn parametric model (the 1995 paper; the parameters per
species are written from the paper and botanical references; no GPL code is copied):
stems of levels 0-3, phyllotaxis through `rotate` of about 137.5 degrees, down-angles,
curve, taper, tropism (photo / gravi), a pruning envelope that fits the target crown, dead
branches, asymmetry from the seed.

* **Trunk and branches:** generalised cylinders with smooth joints (the branch base is
  blended into its parent) and a root flare of 3-5 buttresses scaled with the trunk radius.
  Bark UVs have constant texel density: u around the stem, v = arc length / circumference.
* **Leaves:** twig-cluster cards cropped to an 8-vertex hull of the alpha (overdraw
  -40-60 %), oriented along the parent branch.
* **Baked per vertex:** the wind hierarchy (pivot positions of trunk / branch / twig and the
  level), phase, stiffness, crown-depth AO, crown-sphere normal and a per-leaf random.
* **Vertex format** (36-48 B, packed): position `f32x3`, normal `snorm8x4`, uv
  `unorm16x2`, wind pivot `f16x4`, crown normal `snorm8x4`, ao / phase / stiffness / rand
  `unorm8x4`.
* **Variants:** 8 per archetype per quality level, shared by all instances (random yaw,
  scale and tint on top), generated on the rayon pool when a tile that needs them streams
  in. Cache: `~/.openomsi/cache/vegetation/v<GEN_VERSION>/<archetype>-<variant>-<quality>.bin`;
  the key is the generator version + preset hash + seed. Typical cost 30-80 ms per variant
  on one core; at most about 64 variants per map.
* **LOD chain** (triangles, broadleaf):

| LOD | Triangles | Distance |
|---|---:|---:|
| LOD0 | <= 30 k | <= 40 m |
| LOD1 | <= 8 k | <= 100 m |
| LOD2 | <= 2 k | <= 250 m |
| impostor | 2 | beyond |

Reduction merges leaf clusters into bigger cards, drops level-3 stems and uses fewer
radial segments.

### 4.6 Materials and textures (Phase 3)

* Per archetype: bark (albedo, normal, roughness, AO, height) and a leaf atlas (albedo,
  normal, roughness, thickness).
* Colour: the underside of a leaf is lighter and less saturated.
* Seasons: spring, summer and autumn palettes per species with per-cluster variation.
  Winter: deciduous trees show the full skeleton (leaf cards are not drawn); conifers stay
  green; snow by the up-facing normal through the existing enhanced snow
  (`enhanced.wgsl:885`).
* **Coverage-preserving mips:** for each mip level the alpha is scaled so that the
  coverage at `ALPHA_REF` equals that of mip 0 (Castano 2010). Only for vegetation
  textures (PR 7), so vanilla stays identical.
* BC1 / BC3 through `bc::encode`; the memory is counted in `texture_memory`.
* **Source of the textures: decision (a), section 8.**

### 4.7 Rendering (Phase 4)

* **Pipelines and buffers:** own pipelines in `enhanced`; one instance buffer per (variant,
  LOD) with position, yaw, scale, tint, seed and fade: 32 B per instance.
* **Culling:** on the CPU by 64 m clusters (frustum + distance) on all backends, so it
  works on GLES 3.0 without compute or indirect draws. GPU culling with indirect draws is a
  later optimisation on Vulkan / DX12 / Metal, only if profiles show CPU culling above
  0.5 ms (Thüringer Wald). Hi-Z occlusion is not planned (cost against benefit); noted as
  future work.
* **LOD transitions:** a dithered cross-fade over 10 % of the LOD distance, a screen-space
  4x4 Bayer pattern with complementary patterns for the outgoing and incoming LOD; with
  MSAA through the coverage mask.
* **Octahedral impostors:** hemi-octahedral, 8x8 views (trees are not seen from below),
  baked on the GPU with a render pass (no compute, so it works on GLES): albedo + alpha and
  normal + depth. 128 px per view on High (a 1024x1024 atlas), 64 on Low / Medium, 256 on
  Ultra; cached on disk. They are lit in the same shader as the full model (normal, sun,
  fog, SH ambient) and cast shadows in the far cascade.
* **Shading:** two-sided leaves; transmission (wrap lighting plus view-dependent forward
  scattering towards the sun, warm yellow-green, times the thickness); GGX specular; the
  normal blends towards the crown sphere with distance; vertex AO; cascaded shadows with
  alpha test, so leaves receive self-shadowing. Bark: PBR, POM within 15 m (Ultra / High),
  detail normal.
* **Wind:** a hierarchical extension of `tree_sway`: trunk bend, branch sway with a phase
  delay by hierarchy level, leaf flutter. Strength and direction come from the OMSI weather
  (`Lighting.wind`); gusts are a travelling wave `g(dot(p, wind_dir) - c*t)` (the existing
  `tree_gust_drift`, `lib.rs:2140`).
* **AA:** MSAA + alpha-to-coverage + coverage-preserving mips. The renderer has no TAA, so
  no motion vectors are needed.
* **Mirrors and reflections:** LOD + 1 and no leaf shadows.
* **Night and rain:** the lamp grid lights the crowns like other objects; rain darkens wet
  bark through the existing wetness.
* **Profiling:** a new `OMSI_PROFILE` stage `vegetation` and a GPU timer `vegetation`.

### 4.8 Texture-only mode (Phase 5)

The original geometry is kept (a card or a plant model). A new shader adds translucency,
alpha-to-coverage, coverage-preserving mips, wind by vertex height and a per-instance tint.
For `[tree]` cards, the card picture can be replaced by a baked side view of the matching
procedural tree. This is also the fallback for low confidence (4.2) and for weak
devices (4.9).

### 4.9 Settings (Phase 6)

* `trees = original | textures | realistic` and `tree_quality = low | medium | high |
  ultra` in `settings.cfg` (`omsi-app/src/settings.rs:47-52`), a row each in the Graphics
  tab (`launcher/pages.rs:555`) and the in-game options (`game_lists.rs:2255, 2392-2411`,
  `switch_row` `:1070`, `slider_row` `:1076`); strings in `locales/app.yml`.
* Defaults: desktop Windows / macOS / Linux in `enhanced`: `realistic` / `high`; GLES or
  Android: `textures` (or `realistic` / `low` on strong devices). Vanilla modes ignore both
  settings.
* Debug flags through `omsi_cfg::flags`: `OMSI_TREES` (override the mode) and
  `OMSI_DEBUG_TREES` (classification overlay and log); documented in `DEBUG_FLAGS.md`
  (blessed with `OMSI_FLAGS_BLESS=1 cargo test -p omsi-cfg flags`).

## 5. Performance budgets

Reference: 1440p, a mid GPU (RTX 3060 / RX 6600 class), two worst views: Berlin-Spandau
(street view with the most trees) and Thüringer Wald (forest road).

| Budget | Low | Medium | High | Ultra |
|---|---:|---:|---:|---:|
| Vegetation GPU time, main + shadows | <= 1.0 ms | <= 2.0 ms | <= 3.5 ms | <= 5.0 ms |
| CPU (cull + instance upload), main thread | <= 0.5 ms | <= 0.5 ms | <= 0.5 ms | <= 0.5 ms |
| Meshes in GPU memory | <= 64 MB | <= 64 MB | <= 64 MB | <= 64 MB |
| Vegetation textures + impostors (in `texture_memory`) | <= 48 MB | - | <= 160 MB | - |

* Generation and baking run only on the rayon pool or spread over frames: frames above
  50 ms while streaming: 0.
* With Trees = Original there is no measurable change (within noise) against today.
* Average frame rates are compared with the numbers in `docs/ARCHITECTURE.md` (Spandau
  about 123 fps at 1280x720, 1x MSAA).

## 6. PR plan

| PR | Phase | Content | Size |
|---:|---:|---|---|
| 1 | 0 | `omsi-check --only trees` + this document | S |
| 2 | 1 | `omsi-vegetation` crate: classifier, seed, tests; the inventory gets a confidence column | M |
| 3 | 1 | overrides parser + `MODDING.md` | S |
| 4 | 2 | Weber-Penn skeleton, tube meshing, root flare, determinism tests | L |
| 5 | 2 | leaf cards, baked vertex data, LOD chain | L |
| 6 | 2 | disk cache, generation on the rayon pool tied to tile streaming (not drawn yet) | M |
| 7 | 3 | coverage-preserving mips in `omsi-texture` (only vegetation, to keep vanilla identical; not for existing alpha-tested textures) | S |
| 8 | 3 | bark and leaf textures, seasonal palettes | L |
| 9 | 4 | renderer vegetation pipeline: instancing, CPU culling, LOD + dithered fade, hook in `place_trees` behind the setting | L |
| 10 | 4 | leaf and bark shading (transmission, crown normals, POM) | M |
| 11 | 4 | hierarchical wind | M |
| 12 | 4 | shadows | M |
| 13 | 4 | impostors (bake, cache, draw, shadow) | L |
| 14 | 3/4 | seasons, winter skeletons, snow, wet bark | M |
| 15 | 5 | texture-only mode | M |
| 16 | 6 | settings UI, defaults per backend | M |
| 17 | 7 | verification shots, profiles, docs (`USER_GUIDE`, `MODDING`, `DEBUG_FLAGS`, `CHANGELOG`) | M |

Sizes: S under 300 changed lines, M under 1000, L over that. One change per PR
(`CONTRIBUTING.md`); every PR passes `cargo test --locked --workspace` and
`scripts/size-budget.sh`, and PRs touching the renderer also run
`cargo test --locked --release -p omsi-render shaders_validate_and_match_the_uniforms`.

## 7. Verification

* **Fixed camera points** on Grundorf and Berlin-Spandau, cab and outside, summer and
  winter, day / dusk / night, rendered with
  `openomsi --root "<OMSI 2>" --map maps/Grundorf/global.cfg --enhanced --offscreen out.png
  --time 18:30 --season summer --cam x,y,z,yaw,pitch[,fov]` and `OMSI_SEED=1`.
* **Vanilla identity:** `scripts/golden.sh` (10 scenes at 960x540, `OMSI_SEED=1`, fresh
  `HOME`, `windy_trees=0`): the vanilla scenes stay at 0 px difference. The enhanced
  scenes may change only where the setting is on; with Trees = Original they must match
  today's numbers (`grundorf_enhanced` 0.01 %, `drive_traffic` 1 %). `golden.sh` is not in
  CI and is run by hand before the vegetation PRs merge.
* **Performance:** `OMSI_PROFILE` stage numbers and the GPU timer `vegetation` at the two
  worst views of section 5, before and after.
* **Unit tests:** classifier (table of stock sco + texture pairs), overrides parser,
  generator determinism (same seed, same bytes), cache keys (version, preset, seed).
* **Inventory:** `omsi-check --only trees` before and after, to see which types land in
  which archetype and with which confidence.

## 8. Risks and decisions

**Risks.**

| Risk | Answer |
|---|---|
| Scale on Thüringer Wald (10^5 instances) | impostors + 64 m clusters |
| GLES / Android: no compute, BC missing, so RGBA memory x4-8 | no compute in the baseline path; Low uses smaller atlases |
| First-load generation and impostor bake time | disk cache; bake at most 1 variant per frame |
| Wrong species | overrides + texture-only fallback |
| Overdraw of leaf cards | hull-cropped cards, LOD |
| Shadow cost (three cascades x trees) | one LOD coarser in shadows, impostors in the far cascade |
| `[tree]` used for flowers and small plants | ground-cover words or height below 2 m give `None` |
| The author's composition | target dimensions come from the map |
| Issue #1084, winter look | skeleton without leaves, tested on winter shots |
| Size budget | new files stay under 1500 lines (split `gen` by stage) |

**Decisions.**

* **(a) Texture source:** procedural textures generated at runtime, no binaries in the
  repo. An optional CC0 "vegetation pack" (ambientCG / Poly Haven, licences listed in its
  README, installed through the Mods page) may come later.
* **(b) Overrides file format:** OMSI-style `.cfg`, no new external crate. Compatibility with
  OMSI 2 itself is not a goal; only openOMSI reads the file.
* **(c) `vanilla_plus`:** gets nothing.
