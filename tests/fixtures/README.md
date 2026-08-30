# Third-party test fixtures — provenance

## arduino-* (leonardo, mega-2560, micro, nano, uno-thru-hole)

KiCad schematic/symbol sets for the Arduino board family, authored by
**Carlos Sabogal** (title blocks, 2024) and redistributed here under the
**WTFPL** license each directory carries as `LICENSE.txt` (the upstream
author's chosen license).

Two provenance notes:

- The embedded `lib_symbols` blocks are cached copies of symbols that
  originate in the **KiCad official symbol libraries**
  (CC-BY-SA-4.0 with the KiCad libraries exception). The upstream
  author distributed the whole project under WTFPL; the cached symbols'
  ultimate origin is noted here for completeness.
- These files are used ONLY as parser/importer test fixtures — BHDL
  reads them to exercise the KiCad import path. Nothing from them ships
  in bhdl-stdlib.

The Arduino name is a trademark of Arduino SA; these fixtures describe
the openly published board designs and imply no affiliation.
