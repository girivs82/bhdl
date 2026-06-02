#!/usr/bin/env python3
"""BHDL supply-chain provider — JLCPCB / LCSC (jlcparts dataset).

Turns BHDL's parametric part requirements into real, orderable LCSC parts
(MPN + manufacturer + stock + price), querying an OFFLINE snapshot of the
JLCPCB basic/preferred parts catalogue. Zero-config, no API key, no network
at query time, fully reproducible — the data is a local CSV snapshot.

Data source: CDFER/jlcpcb-parts-database (MIT), derived from yaqwsx/jlcparts.
  full SQLite (~1GB):  https://cdfer.github.io/jlcpcb-parts-database/jlcpcb-components.sqlite3
  basic+preferred CSV: https://cdfer.github.io/jlcpcb-parts-database/jlcpcb-components-basic-preferred.csv  (~3.6MB)

The CSV (basic + preferred = the no-extra-fee, in-stock assembly parts) is
the right default for cost-optimized assembly. Point at it via
$BHDL_JLCPARTS_CSV, or pass a path as argv[1]; for full coverage a sibling
provider can query the SQLite instead.

Protocol (JSON over stdin/stdout), aligned with bhdl-analyzer plugin.rs
PluginResponse/PluginSelection:

  stdin:  {"protocol":1, "requirements":[
             {"class_index":0, "class":"resistor", "value":10000.0,
              "package":"0603", "tolerance_pct":1.0} ...]}
  stdout: {"protocol_version":"1", "selections":[
             {"class_index":0, "mpn":"...", "manufacturer":"...", "vendor":"LCSC",
              "vendor_sku":"C25804", "stock":100000, "unit_price":0.0008,
              "currency":"USD"} ...],
           "warnings":[...]}

`value` is SI base units (Ω / F / H). A requirement with no match yields a
selection carrying only `index` + `error`.
"""
import csv
import json
import os
import re
import sys

# Map BHDL normalized class -> the jlcparts `category` column value.
CLASS_TO_CATEGORY = {
    "resistor": "Resistors",
    "capacitor": "Capacitors",
    "inductor": "Inductors/Coils/Transformers",
}

# SI prefix multipliers for value tokens in the `description` text.
PREFIX = {
    "p": 1e-12, "n": 1e-9, "u": 1e-6, "µ": 1e-6, "μ": 1e-6,
    "m": 1e-3, "": 1.0, "k": 1e3, "K": 1e3, "M": 1e6, "G": 1e9,
}
# Unit letter per class, as it appears in the description.
UNIT = {"resistor": "Ω", "capacitor": "F", "inductor": "H"}


def parse_value(token_unit, text):
    """Extract the first `<number><prefix><unit>` value from `text` as an
    SI-base float, e.g. "510kΩ"->510e3, "100nF"->100e-9, "10µH"->10e-6.
    `token_unit` is the dimension unit letter (Ω/F/H)."""
    # number, optional SI prefix, the unit letter (Ω may be written R-less)
    pat = re.compile(r"(\d+(?:\.\d+)?)\s*([pnuµμmkKMG]?)" + re.escape(token_unit))
    m = pat.search(text)
    if not m:
        return None
    num = float(m.group(1))
    return num * PREFIX.get(m.group(2), 1.0)


def first_tier_price(price_json):
    """Lowest-quantity unit price from the tiered `price` JSON array."""
    try:
        tiers = json.loads(price_json)
        if tiers:
            return float(tiers[0].get("price"))
    except Exception:
        pass
    return None


def load_rows(csv_path):
    with open(csv_path, newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def select(req, rows):
    cls = req.get("class")
    category = CLASS_TO_CATEGORY.get(cls)
    unit = UNIT.get(cls)
    if category is None or unit is None:
        return {"class_index": req["class_index"], "error": f"unsupported class '{cls}'"}

    target = req.get("value")
    want_pkg = (req.get("package") or "").strip().lower()
    tol = req.get("tolerance_pct", 2.0) / 100.0

    candidates = []
    for r in rows:
        if r.get("category") != category:
            continue
        try:
            if int(r.get("stock") or 0) <= 0:
                continue
        except ValueError:
            continue
        if want_pkg and (r.get("package") or "").strip().lower() != want_pkg:
            continue
        if target is not None:
            v = parse_value(unit, r.get("description") or "")
            if v is None or v <= 0:
                continue
            # ratio tolerance (values are E-series-snapped upstream)
            if abs(v - target) > target * max(tol, 1e-9):
                continue
        # rank key: basic first, then preferred, then most stock, cheapest
        basic = 1 if (r.get("basic") == "1") else 0
        pref = 1 if (r.get("preferred") == "1") else 0
        stock = int(r.get("stock") or 0)
        price = first_tier_price(r.get("price") or "") or 9e9
        candidates.append((basic, pref, stock, -price, r))

    if not candidates:
        return {"class_index": req["class_index"],
                "error": f"no in-stock {cls} matching value/package in catalogue"}

    candidates.sort(key=lambda t: (t[0], t[1], t[2], t[3]), reverse=True)
    r = candidates[0][4]
    return {
        "class_index": req["class_index"],
        "mpn": r.get("mfr") or None,
        "manufacturer": r.get("manufacturer") or None,
        "vendor": "LCSC",
        "vendor_sku": "C" + str(r.get("lcsc")),
        "stock": int(r.get("stock") or 0),
        "unit_price": first_tier_price(r.get("price") or ""),
        "currency": "USD",
        "note": "basic" if r.get("basic") == "1" else ("preferred" if r.get("preferred") == "1" else None),
    }


def main():
    csv_path = (sys.argv[1] if len(sys.argv) > 1
                else os.environ.get("BHDL_JLCPARTS_CSV"))
    req = json.load(sys.stdin)
    warnings = []
    if not csv_path or not os.path.exists(csv_path):
        # No data: emit a well-formed empty response with a warning, so the
        # caller falls back to catalogue defaults rather than failing.
        print(json.dumps({
            "protocol_version": "1", "selections": [],
            "warnings": ["jlcparts CSV not found; set $BHDL_JLCPARTS_CSV "
                         "(download from cdfer.github.io/jlcpcb-parts-database)"],
        }))
        return
    rows = load_rows(csv_path)
    selections = [select(r, rows) for r in req.get("requirements", [])]
    print(json.dumps({"protocol_version": "1", "selections": selections, "warnings": warnings}))


if __name__ == "__main__":
    main()
