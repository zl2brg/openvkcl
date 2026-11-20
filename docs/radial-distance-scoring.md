# Radial distance scoring

Five contests set `kind = "distance"` and score each contact from the distance between the two stations. They are the NZART VHF DX Weekend, VHF Field Day, Low Band Contest and Microwave Contest, and the WIA VHF-UHF Field Day. The measurement and the arithmetic live in [`src-tauri/src/geo.rs`](../src-tauri/src/geo.rs) and in `basic_points` / `points` in [`src-tauri/src/contest/engine.rs`](../src-tauri/src/contest/engine.rs). The brackets, factors and bonuses are set in the contest files under [`src-tauri/contests`](../src-tauri/contests).

The other contests use fixed points and never store a distance: NZART Jock White Memorial Field Day, NZART Memorial Contest, NZART Sangster Shield, WIA Remembrance Day, WIA Trans-Tasman Low-Band, WIA Harry Angel Memorial 80m Sprint, and WIA John Moyle Memorial Field Day.

## Distance

The distance is the great-circle distance between the centres of the two Maidenhead locators. `distance_km` uses the haversine formula with an Earth radius of 6371 km.

Both locators must be at least 6 characters and valid. `is_valid` accepts 4, 6, 8 and 10 character Maidenhead locators, and a 4-character square fails the length check along with any ill-formed locator. In either case the contact scores 0 and no distance is stored. A short or invalid locator is a warning (`exchange_warning`); the contact still counts as worked. The warning reads "Received locator must be at least 6 characters", or the same for the station's own locator.

A contact whose two locators share a centre is 0 km. On a points-per-kilometre band that is 0 points. On a table band, 0 km falls in the first bracket.

Dupes and invalid contacts (a callsign, band or mode the contest does not accept) are dropped in `evaluate` before points are calculated, so they show no distance.

## From a distance to a basic score

A band listed in `band_table` uses the table. Every other band uses kilometres times that band's factor. The table wins when a band appears in both.

### Table

`band_table` rows are `[upper bound in km, points]`, read in order. The contact takes the first row whose upper bound is strictly greater than the distance. A distance of exactly 25.0 km falls through the 25 km row and takes the 50 km row. The final `inf` row covers everything at or beyond 5000 km.

NZART VHF DX Weekend, VHF Field Day and Low Band share this Table 1. On 6 m the 600 km, 800 km and 1200 km rows are all 25 points, so 6 m stays at 25 from 500 km up to (and not including) 1200 km.

| Upper bound (km) | 6 m | 2 m | 70 cm |
| --- | ---: | ---: | ---: |
| 25 | 1 | 1 | 2 |
| 50 | 2 | 2 | 3 |
| 75 | 3 | 3 | 5 |
| 100 | 5 | 5 | 7 |
| 150 | 7 | 7 | 10 |
| 200 | 10 | 10 | 15 |
| 300 | 12 | 12 | 20 |
| 400 | 15 | 15 | 30 |
| 500 | 20 | 20 | 40 |
| 600 | 25 | 25 | 50 |
| 800 | 25 | 40 | 75 |
| 1200 | 25 | 50 | 100 |
| 2000 | 35 | 75 | 150 |
| 3000 | 50 | 125 | 250 |
| 4000 | 60 | 175 | 350 |
| 5000 | 80 | 225 | 450 |
| inf | 100 | 250 | 500 |

### Kilometres times a band factor

On a band with no table row, the basic score is the distance in kilometres times `band_factor` for that band. A band missing from the factor map scores 0.

Where a taper is set, the listed bands compress the tail of the distance before that multiplication. Once the distance exceeds `taper_km`, the excess kilometres are divided by `taper_divisor`:

```text
effective_km = taper_km + (km - taper_km) / taper_divisor
```

A distance equal to `taper_km` is unchanged. The band factor then multiplies `effective_km`, including the tapered part. On the WIA 6 m, 2 m and 70 cm bands, `taper_km` is 700 and `taper_divisor` is 100:

```text
effective_km = 700 + (km - 700) / 100
```

## Rounding and multipliers

For a contact that has a distance, the engine applies these in order:

1. Round the basic score. Every NZART VHF contest sets `round = "nearest"`. WIA leaves the score as calculated (`round` defaults to none). Nearest rounding first snaps the value to the nearest 10⁻⁹, so a product that should land on .5 is not left a hair under the halfway point by binary floating point, then rounds half away from zero.
2. Multiply by the mode factor. NZART RTTY (`RY`) and digital (`DG`) are ×1.5. Phone and CW are absent from the mode map, so they stay ×1. WIA has no mode map, so every mode stays ×1. This product is not rounded on its own.
3. Multiply by the bonus product, then round again with the same rule as step 1. NZART QRP (`CATEGORY-POWER = QRP`) is ×1.5 on the bands ticked in the setup. A field station (`CATEGORY-STATION = PORTABLE`) is ×1.2 on every contact. The two multiply, so a ticked band with both is ×1.8, and an unticked band with only the field station is ×1.2. A log that has QRP set and no band list, from before the per-band checkboxes, takes the QRP bonus on every band. WIA has no scoring bonus, so this step multiplies by 1 and, with no rounding rule, leaves the contact points unchanged.
4. Each score group's claimed score is the ceiling of that group's point sum (`evaluate`, after `sum_points`). NZART defines one group per band, and its contact points are already whole numbers, so the ceiling does not change a band total. The contest total is the sum of those band scores. WIA defines no groups, keeps fractional contact points, and ceilings only the one total for the whole log.

## NZART VHF DX Weekend, Field Day, and Low Band

[`nzart-vhf-dx.toml`](../src-tauri/contests/nzart-vhf-dx.toml), [`nzart-vhf-fd.toml`](../src-tauri/contests/nzart-vhf-fd.toml) and [`nzart-vhf-lowband.toml`](../src-tauri/contests/nzart-vhf-lowband.toml) share Table 1 for 6 m, 2 m and 70 cm.

DX Weekend and Field Day also score 33 cm and up at points per kilometre. There is no taper. Low Band has only the three table bands.

| Band | Points per km |
| --- | ---: |
| 33 cm | 0.3 |
| 1.2 GHz | 0.2 |
| 2.3 GHz | 0.5 |
| 3.4 GHz and above (5.7, 10, 24, 47, 75, 122, 134 and 241 GHz) | 1 |

Each band is its own score group, with its own claimed score. Locators longer than 6 characters are used at full precision (`max_locator_chars` is unset).

**Example.** The DX Weekend test works RF73hc to RE66hq, 733 km, on 6 m digital. 733 km is under the 800 km bound, so the table gives 25. The basic score is already a whole number. Digital multiplies by 1.5: 25 × 1.5 = 37.5. With no bonus, the second rounding takes 37.5 to 38. With QRP ticked for 6 m and a field station, the bonus product is ×1.8 and it is applied to the unrounded 37.5: 37.5 × 1.8 = 67.5, which rounds to 68.

## NZART Microwave

[`nzart-vhf-microwave.toml`](../src-tauri/contests/nzart-vhf-microwave.toml) scores every band in points per kilometre, using the same factors as DX Weekend and Field Day, from 33 cm at ×0.3 through ×1 from 3.4 GHz up. Each band is its own score group.

Locators of 8 and 10 characters are used at full precision. The contest rules ask for about 1 km precision above 2.4 GHz, which an 8-character locator supplies. The same nearest-rounding, RTTY/digital ×1.5, QRP ×1.5 and field-station ×1.2 rules apply. There is no taper.

## WIA VHF-UHF Field Day

[`wia-vhfuhf.toml`](../src-tauri/contests/wia-vhfuhf.toml) measures between 6-character sub-square centres even when a longer locator was logged (`max_locator_chars = 6`). Two stations in the same sub-square therefore measure 0 km, and 0 km times any band factor is 0 points.

Points are tapered kilometres times the band factor. The 700 km taper applies only on 6 m, 2 m and 70 cm. Higher bands use the raw distance. There is no per-contact rounding, no mode factor and no scoring bonus. The log has one claimed total, the ceiling of the sum of the contact points.

| Band | Factor |
| --- | ---: |
| 6 m | 1.7 |
| 2 m | 1 |
| 70 cm | 2.7 |
| 1.2 GHz | 3.7 |
| 2.3 GHz | 4.4 |
| 3.4 GHz | 5.4 |
| 5.7 GHz | 6.4 |
| 10 GHz | 7.4 |
| 24 GHz and above (47, 75, 122, 134 and 241 GHz) | 10 |

**Example.** The WIA distance test works QF22le to QF56od on 70 cm, about 714.7 km. That exceeds 700 km, so the kilometres are tapered and then multiplied by 2.7:

```text
effective_km = 700 + (km - 700) / 100
points = effective_km × 2.7
```

That product is about 1890.4 and is stored on the contact as calculated. The claimed score ceilings only the sum for the whole log.
