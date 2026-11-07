# Fidelity log

One row per Fidelity Loop iteration (STEP_BY_STEP_PLAN Stage E): region, property, before -> after, the change.
Numbers are `tools/fidelity.ps1` rows; "mockup px" are physical pixels at the 1.5 scale.

## Ticket 07: paper, shadow, tapes

| # | Region | Property | Before -> after | Change |
|---|---|---|---|---|
| 1 | tape_pink | bbox y0 (hover) | +4 -> +2 (idle +4 -> +1) | The tilt lifts the paper's top-left corner above the window top and the pink tape reaches 3 logical px beyond it, so the window clipped it. Paper drawn `paper-shift-y` = 1.8 px below the window center (the 40 px margin is split 42 / 38). Registration aligns the paper centers, so the paper bbox rows are unchanged |
| 2 | paper | height (hover) | 648.866 expected, +1.122 -> +0.096 | fidelity.ps1 snaps the hover paper at 362.9 x 432.6 (the mockup's measured size) instead of 363 x 433; the idle one at 362.9 x 383.7 |
| 3 | shadow | ring MAE | 2.19 / 2.41 (idle / hover) | Baseline: 4 layers fitted by eye in an earlier attempt |
| 4 | shadow | ring MAE | 2.77 -> 2.29 | 6 layers fitted to side profiles of the mockups (`shadow-growths`, `shadow-colors`, `shadow-offset-y` 10.7). Still above 2.0: staircase and misregistration |
| 5 | shadow | ring MAE | 2.29 -> 1.5 (still failing before this) | Found by emulating the metric: about 1.2 of the 2.1 came from the tape pixels in the ring, not the shadow. The stripes of the tapes were out of phase with the mockups (99.7% of tape pixels agree once shifted by 0.295 of the pitch) |
| 6 | tape_pink, tape_blue | stripe phase | 0 -> 0.295 | New spec value `layout.tapes.stripe_phase`; tapegen adds it to the stripe coordinate; images regenerated |
| 7 | shadow | ring MAE | 1.24 (6 layers) -> 1.51 / 1.53 (4 layers) | Back to 4 layers as in ADR 0001 (cost of a full repaint at 150%: 6 ms vs 15 ms for 8): growths 12.3 / 6.9 / 3.1 / -3.2 px, alphas 5 / 5 / 8 / 25 of 255, offset 10.8 px |
| 8 | tape_blue | bbox y1 (idle) | -4 | Not fixed: the idle mockup's blue tape ends 4 mockup px below the window bottom (margin 38.2 after the shift). Needs a 42 px margin; kept in the baseline |

Final: paper, shadow, tape_pink, tape_blue pass on hover; on idle everything but tape_blue y1 passes; invariants `tilt` and `tapes` pass (1.5 deg at 1.5x paper size, tape size and corner offsets within 1 px).
