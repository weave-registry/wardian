# Unit converter

A Wardian **module app**: 32 everyday conversions, each one a WebAssembly function of numbers.
There is no page. Wardian reads the module's exports and draws a card for each function, with an
input per parameter and a Run button.

Why a module: every conversion takes numbers and returns a number. That is exactly what a module
app shows without help, so the whole interface comes from `src/lib.rs`. No HTML, no JavaScript.

| File | What it is |
|---|---|
| `src/lib.rs` | The conversions. Each `#[no_mangle] pub extern "C" fn` becomes a card in Wardian. Unit tests at the end. |
| `app.wasm` | The module Wardian runs, about 2 KB. Built from `src/lib.rs`. |
| `app.json` | The title and the description in the app list. |
| `build.sh` | Rebuilds `app.wasm`. |

| Group | Functions |
|---|---|
| Temperature | `c_to_f`, `f_to_c`, `c_to_k`, `k_to_c` |
| Length | `km_to_miles`, `miles_to_km`, `m_to_feet`, `feet_to_m`, `cm_to_inches`, `inches_to_cm`, `feet_inches_to_cm(feet, inches)` |
| Mass | `kg_to_lb`, `lb_to_kg`, `g_to_oz`, `oz_to_g` |
| Volume (US) | `liters_to_gallons`, `gallons_to_liters`, `ml_to_floz`, `floz_to_ml` |
| Speed and fuel | `kmh_to_mph`, `mph_to_kmh`, `mpg_to_l_per_100km`, `l_per_100km_to_mpg` |
| Pressure | `kpa_to_psi`, `psi_to_kpa`, `bar_to_psi` |
| Power and energy | `kw_to_hp`, `hp_to_kw`, `kcal_to_kj`, `kj_to_kcal` |
| Body | `bmi_kg_m(weight_kg, height_m)`, `bmi_lb_in(weight_lb, height_in)` |

How data moves: you type a number in a card and press Run (or Enter). Wardian calls the function
with a JavaScript number, the module returns a 64-bit float, and Wardian shows it under the card.
Nothing leaves the browser.

Names carry the units, because Wardian labels the inputs only "arg 1", "arg 2". So
`km_to_miles` takes kilometres, and a function with two inputs names them in order:
`bmi_kg_m` takes the weight in kilograms first, then the height in metres.

Limits:

- Results are rounded to 4 decimal places (BMI to 1), so 37 °C shows as 98.6 and not
  98.60000000000001.
- Gallons and fluid ounces are US units. The factors are the exact legal definitions
  (1 mile = 1.609344 km, 1 lb = 0.45359237 kg, 1 US gallon = 3.785411784 L).
- `bmi_kg_m` returns 0 when the height is 0 or less. The fuel functions return 0 for 0 or less.
- Horsepower is mechanical horsepower (745.7 W), not metric.

Change it: edit `src/lib.rs`, run `cargo test`, run `./build.sh`, then `wardian check .`.
