//! Unit converter: everyday conversions as plain functions of numbers.
//!
//! Wardian draws one card per exported function, with one input per parameter, so each
//! name says what goes in and what comes out: `km_to_miles` takes kilometres and returns miles.
//! Functions with two inputs name both, in order: `bmi_kg_m(weight_kg, height_m)`.
//!
//! Results are rounded to 4 decimal places (BMI to 1), so 37 °C shows as 98.6 °F and not
//! 98.60000000000001. Volumes use US units: 1 US gallon = 3.785411784 litres.

/// Rounds to `places` decimal places, so the result prints without float noise.
fn round(x: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (x * f).round() / f
}

fn r4(x: f64) -> f64 {
    round(x, 4)
}

// Exact definitions, from the international yard and pound agreement and NIST.
const KM_PER_MILE: f64 = 1.609344;
const M_PER_FOOT: f64 = 0.3048;
const CM_PER_INCH: f64 = 2.54;
const KG_PER_LB: f64 = 0.45359237;
const G_PER_OZ: f64 = 28.349523125;
const L_PER_US_GALLON: f64 = 3.785411784;
const ML_PER_US_FLOZ: f64 = 29.5735295625;
const KPA_PER_PSI: f64 = 6.894757293168361;
const W_PER_HP: f64 = 745.69987158227022; // mechanical horsepower
const J_PER_KCAL: f64 = 4184.0;

// Temperature

/// Celsius to Fahrenheit.
#[no_mangle]
pub extern "C" fn c_to_f(c: f64) -> f64 {
    r4(c * 9.0 / 5.0 + 32.0)
}

/// Fahrenheit to Celsius.
#[no_mangle]
pub extern "C" fn f_to_c(f: f64) -> f64 {
    r4((f - 32.0) * 5.0 / 9.0)
}

/// Celsius to kelvin.
#[no_mangle]
pub extern "C" fn c_to_k(c: f64) -> f64 {
    r4(c + 273.15)
}

/// Kelvin to Celsius.
#[no_mangle]
pub extern "C" fn k_to_c(k: f64) -> f64 {
    r4(k - 273.15)
}

// Length

/// Kilometres to miles.
#[no_mangle]
pub extern "C" fn km_to_miles(km: f64) -> f64 {
    r4(km / KM_PER_MILE)
}

/// Miles to kilometres.
#[no_mangle]
pub extern "C" fn miles_to_km(miles: f64) -> f64 {
    r4(miles * KM_PER_MILE)
}

/// Metres to feet.
#[no_mangle]
pub extern "C" fn m_to_feet(m: f64) -> f64 {
    r4(m / M_PER_FOOT)
}

/// Feet to metres.
#[no_mangle]
pub extern "C" fn feet_to_m(feet: f64) -> f64 {
    r4(feet * M_PER_FOOT)
}

/// Centimetres to inches.
#[no_mangle]
pub extern "C" fn cm_to_inches(cm: f64) -> f64 {
    r4(cm / CM_PER_INCH)
}

/// Inches to centimetres.
#[no_mangle]
pub extern "C" fn inches_to_cm(inches: f64) -> f64 {
    r4(inches * CM_PER_INCH)
}

/// A height in feet and inches (5, 11) to centimetres.
#[no_mangle]
pub extern "C" fn feet_inches_to_cm(feet: f64, inches: f64) -> f64 {
    r4((feet * 12.0 + inches) * CM_PER_INCH)
}

// Mass

/// Kilograms to pounds.
#[no_mangle]
pub extern "C" fn kg_to_lb(kg: f64) -> f64 {
    r4(kg / KG_PER_LB)
}

/// Pounds to kilograms.
#[no_mangle]
pub extern "C" fn lb_to_kg(lb: f64) -> f64 {
    r4(lb * KG_PER_LB)
}

/// Grams to ounces (avoirdupois).
#[no_mangle]
pub extern "C" fn g_to_oz(g: f64) -> f64 {
    r4(g / G_PER_OZ)
}

/// Ounces (avoirdupois) to grams.
#[no_mangle]
pub extern "C" fn oz_to_g(oz: f64) -> f64 {
    r4(oz * G_PER_OZ)
}

// Volume (US units)

/// Litres to US gallons.
#[no_mangle]
pub extern "C" fn liters_to_gallons(liters: f64) -> f64 {
    r4(liters / L_PER_US_GALLON)
}

/// US gallons to litres.
#[no_mangle]
pub extern "C" fn gallons_to_liters(gallons: f64) -> f64 {
    r4(gallons * L_PER_US_GALLON)
}

/// Millilitres to US fluid ounces.
#[no_mangle]
pub extern "C" fn ml_to_floz(ml: f64) -> f64 {
    r4(ml / ML_PER_US_FLOZ)
}

/// US fluid ounces to millilitres.
#[no_mangle]
pub extern "C" fn floz_to_ml(floz: f64) -> f64 {
    r4(floz * ML_PER_US_FLOZ)
}

// Speed and fuel

/// Kilometres per hour to miles per hour.
#[no_mangle]
pub extern "C" fn kmh_to_mph(kmh: f64) -> f64 {
    r4(kmh / KM_PER_MILE)
}

/// Miles per hour to kilometres per hour.
#[no_mangle]
pub extern "C" fn mph_to_kmh(mph: f64) -> f64 {
    r4(mph * KM_PER_MILE)
}

/// US miles per gallon to litres per 100 km. The same formula works both ways.
#[no_mangle]
pub extern "C" fn mpg_to_l_per_100km(mpg: f64) -> f64 {
    if mpg <= 0.0 {
        return 0.0;
    }
    r4(100.0 * L_PER_US_GALLON / (mpg * KM_PER_MILE))
}

/// Litres per 100 km to US miles per gallon.
#[no_mangle]
pub extern "C" fn l_per_100km_to_mpg(l_per_100km: f64) -> f64 {
    mpg_to_l_per_100km(l_per_100km)
}

// Pressure

/// Kilopascals to pounds per square inch (tyre pressure).
#[no_mangle]
pub extern "C" fn kpa_to_psi(kpa: f64) -> f64 {
    r4(kpa / KPA_PER_PSI)
}

/// Pounds per square inch to kilopascals.
#[no_mangle]
pub extern "C" fn psi_to_kpa(psi: f64) -> f64 {
    r4(psi * KPA_PER_PSI)
}

/// Bar to pounds per square inch.
#[no_mangle]
pub extern "C" fn bar_to_psi(bar: f64) -> f64 {
    r4(bar * 100.0 / KPA_PER_PSI)
}

// Power and energy

/// Kilowatts to mechanical horsepower.
#[no_mangle]
pub extern "C" fn kw_to_hp(kw: f64) -> f64 {
    r4(kw * 1000.0 / W_PER_HP)
}

/// Mechanical horsepower to kilowatts.
#[no_mangle]
pub extern "C" fn hp_to_kw(hp: f64) -> f64 {
    r4(hp * W_PER_HP / 1000.0)
}

/// Food calories (kcal) to kilojoules.
#[no_mangle]
pub extern "C" fn kcal_to_kj(kcal: f64) -> f64 {
    r4(kcal * J_PER_KCAL / 1000.0)
}

/// Kilojoules to food calories (kcal).
#[no_mangle]
pub extern "C" fn kj_to_kcal(kj: f64) -> f64 {
    r4(kj * 1000.0 / J_PER_KCAL)
}

// Body

/// Body mass index from weight in kilograms and height in metres, to 1 decimal place.
/// Returns 0 when the height is not above zero.
#[no_mangle]
pub extern "C" fn bmi_kg_m(weight_kg: f64, height_m: f64) -> f64 {
    if height_m <= 0.0 {
        return 0.0;
    }
    round(weight_kg / (height_m * height_m), 1)
}

/// Body mass index from weight in pounds and height in inches, to 1 decimal place.
#[no_mangle]
pub extern "C" fn bmi_lb_in(weight_lb: f64, height_in: f64) -> f64 {
    bmi_kg_m(weight_lb * KG_PER_LB, height_in * CM_PER_INCH / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(c_to_f(37.0), 98.6);
        assert_eq!(f_to_c(212.0), 100.0);
        assert_eq!(c_to_k(0.0), 273.15);
        assert_eq!(km_to_miles(42.195), 26.2188);
        assert_eq!(miles_to_km(1.0), 1.6093);
        assert_eq!(kg_to_lb(1.0), 2.2046);
        assert_eq!(lb_to_kg(1.0), 0.4536);
        assert_eq!(gallons_to_liters(1.0), 3.7854);
        assert_eq!(kpa_to_psi(220.0), 31.9083);
        assert_eq!(feet_inches_to_cm(5.0, 11.0), 180.34);
        assert_eq!(mpg_to_l_per_100km(30.0), 7.8405);
        assert_eq!(l_per_100km_to_mpg(5.0), 47.0429);
        assert_eq!(bmi_kg_m(70.0, 1.75), 22.9);
        assert_eq!(bmi_kg_m(70.0, 0.0), 0.0);
        assert_eq!(bmi_lb_in(154.0, 69.0), 22.7);
    }
}
