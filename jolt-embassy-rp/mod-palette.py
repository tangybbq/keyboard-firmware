#!/usr/bin/env python3
"""Generate the Taipo modifier indicator palette, `src/leds/mod_palette.rs`.

The indicator shows which modifiers Taipo is holding, and whether they are sticky.  Both are
encoded in one LED: the 15 non-empty modifier sets each get their own chromaticity, all at the
same Oklab lightness, and sticky moves the whole palette between two lightnesses.  Keeping the
lightness constant within a family is what makes that work -- it leaves the lightness axis free to
carry the sticky bit, so the sticky signal stays much larger than the differences between the
modifier sets.

Two ways of choosing the 15 chromaticities are generated, because they trade against each other in
a way no number settles:

  packed  Maximum-minimum-distance packing of 15 points on the constant-lightness plane.  The most
          separable, but the 15 colors have no relationship to the modifiers, so they have to be
          learned individually.  Subsets are assigned to colors so that a combination lands near
          its parts, which helps, but a 4-bit lattice does not fit on a 2-D plane.

  sums    Four base colors, one per modifier, added in linear light and pulled back to the
          constant lightness.  Compositional -- learn four colors and read the rest -- at maybe
          two thirds of the separation.

Which is better is a question about learning, not about color distance, so it is a thing to look
at on the hardware.  Set `MOD_PALETTE` in `src/leds/manager.rs` to switch.

Usage:
    uv run --with numpy python mod-palette.py            # regenerate the Rust table
    uv run --with numpy python mod-palette.py --preview  # show it in the terminal too
"""

import argparse
import itertools
import subprocess
import sys

import numpy as np

from led_palette import (fit_chroma, in_gamut, lin2oklab, lin2srgb, min_pair, oklab2lin,
                         slice_candidates, pack)

# The Mods bits, from bbq-keyboard's `Mods` bitflags.  The palette is indexed by these directly.
MODS = [("CONTROL", 0x1), ("SHIFT", 0x2), ("ALT", 0x4), ("GUI", 0x8)]

# Every non-empty modifier set, as a bit index into the generated tables.
SETS = [i for i in range(1, 16)]


def bits(i):
    return [b for b, (_, m) in enumerate(MODS) if i & m]


def name(i):
    return "+".join(n[0] for n, m in MODS if i & m)


# ---------------------------------------------------------------------------
# The two ways of choosing the chromaticities.
# ---------------------------------------------------------------------------


def packed_palette(l_hi):
    """15 points packed as far apart as they will go on the constant-lightness plane."""
    _, p = pack(slice_candidates(l_hi), 15)
    ab = p[:, 1:]
    return ab[np.argsort(np.arctan2(ab[:, 1], ab[:, 0]))]


def sums_palette(l_hi, seed=3, trials=200):
    """Four base chromaticities whose 15 subset sums separate as well as they can.

    The sums are taken in linear light, which is what "mixing" means, and each is then put back on
    the constant-lightness plane, keeping its hue and pulling the chroma in until it fits.
    """
    subsets = [tuple(bits(i)) for i in SETS]

    def build(params):
        angles, chroma = params[:4] % (2 * np.pi), np.clip(params[4:], 0.01, 0.40)
        ab = np.column_stack([chroma * np.cos(angles), chroma * np.sin(angles)])
        base = np.column_stack([np.full(4, l_hi), ab])
        if not in_gamut(base).all():
            return None
        out = []
        for s in subsets:
            lab = lin2oklab(oklab2lin(base)[list(s)].sum(axis=0)[None])[0]
            d = np.array([lab[1], lab[2]])
            k = 1.0
            while k > 1e-4 and not in_gamut(np.array([[l_hi, d[0] * k, d[1] * k]]))[0]:
                k *= 0.97
            out.append(d * k)
        return np.array(out)

    def score(params):
        ab = build(params)
        if ab is None:
            return -1.0
        return min_pair(np.column_stack([np.full(15, l_hi), ab]))

    rng = np.random.default_rng(seed)
    best, best_s = None, -1.0
    for _ in range(trials):
        p = np.concatenate([rng.uniform(0, 2 * np.pi, 4), rng.uniform(0.05, 0.30, 4)])
        s, step = score(p), 0.25
        for _ in range(300):
            q = p + rng.normal(0, step, 8)
            t = score(q)
            if t > s:
                s, p = t, q
            else:
                step *= 0.995
        if s > best_s:
            best_s, best = s, build(p)
    # Already in subset order; `sums` has no assignment freedom, which is the point of it.
    return best


# ---------------------------------------------------------------------------
# Assigning modifier sets to the packed colors.
# ---------------------------------------------------------------------------


def assign(ab, seed=11, restarts=60):
    """Order the packed colors so a combination sits near the colors of its parts.

    The packing says nothing about which modifier set gets which color, and that choice is what
    decides whether the palette can be read rather than memorised.  Put each combination as close
    as possible to the average of its singletons.  A 4-bit lattice does not embed in a plane, so
    this only reduces the damage.  It cannot change how separable the palette is: permuting the
    assignment does not move any of the points.
    """
    singles = [i for i in SETS if bin(i).count("1") == 1]
    combos = [i for i in SETS if bin(i).count("1") > 1]

    def cost(order):
        pos = {s: ab[order.index(s)] for s in SETS}
        total = 0.0
        for c in combos:
            parts = [pos[1 << b] for b in bits(c)]
            total += np.linalg.norm(pos[c] - np.mean(parts, axis=0))
        # Keep the singletons spread out, so the averages they anchor are spread out too.
        for a, b in itertools.combinations(singles, 2):
            total -= 0.35 * np.linalg.norm(pos[a] - pos[b])
        return total

    rng = np.random.default_rng(seed)
    best, best_c = None, np.inf
    for _ in range(restarts):
        order = list(SETS)
        rng.shuffle(order)
        c = cost(order)
        improved = True
        while improved:
            improved = False
            for i, j in itertools.combinations(range(15), 2):
                order[i], order[j] = order[j], order[i]
                t = cost(order)
                if t < c - 1e-12:
                    c, improved = t, True
                else:
                    order[i], order[j] = order[j], order[i]
        if c < best_c:
            best_c, best = c, list(order)
    # `best[k]` is the modifier set holding color k; invert to index colors by set.
    return {s: ab[best.index(s)] for s in SETS}


# ---------------------------------------------------------------------------
# Turning Oklab into what goes on the wire.
# ---------------------------------------------------------------------------


def device(ab_by_set, l_hi, l_lo, budget):
    """Both families as 8-bit ws2812 values, scaled so the largest channel is `budget`.

    The ws2812's PWM is linear in light, so these are linear values with no gamma encoding.  The
    scaling is a plain multiply, which leaves every chromaticity untouched.
    """
    ab = np.array([ab_by_set[s] for s in SETS])
    hi = np.column_stack([np.full(15, l_hi), ab])
    lo = fit_chroma(ab, l_lo)
    lin = np.vstack([oklab2lin(hi), oklab2lin(lo)])
    k = budget / (lin.max() * 255.0)
    pwm = np.round(np.clip(lin * 255.0 * k, 0, 255)).astype(int)
    return hi, lo, pwm[:15], pwm[15:], k


def quality(hi, lo, pwm_hi, pwm_lo, k):
    """What separation actually survives the 8-bit quantization, in Oklab."""
    back = lin2oklab(np.vstack([pwm_hi, pwm_lo]) / (255.0 * k))
    states = np.vstack([back, np.zeros((1, 3))])
    ideal = min_pair(np.vstack([hi, lo, np.zeros((1, 3))]))
    dup = 30 - len(set(map(tuple, np.vstack([pwm_hi, pwm_lo]).tolist())))
    return ideal, min_pair(states), dup


# ---------------------------------------------------------------------------
# Output.
# ---------------------------------------------------------------------------


def swatch(pwm, k):
    """A terminal block showing roughly what the color is, brightened to be visible."""
    srgb = np.atleast_1d(lin2srgb(np.clip(np.asarray(pwm, float) / (255.0 * k), 0, 1))).ravel()
    r, g, b = (int(round(float(v) * 255)) for v in srgb[:3])
    return f"\x1b[48;2;{r};{g};{b}m    \x1b[0m"


def preview(tag, hi, lo, pwm_hi, pwm_lo, k):
    ideal, real, dup = quality(hi, lo, pwm_hi, pwm_lo, k)
    print(f"\n{tag}: min dE {ideal:.4f} ideal, {real:.4f} after 8-bit"
          f"{'  *** COLLIDING STATES ***' if dup else ''}")
    print(f"  {'modifiers':<20} {'sticky (bright)':<28} one-shot (dim)")
    for n, s in enumerate(SETS):
        h, d = [int(v) for v in pwm_hi[n]], [int(v) for v in pwm_lo[n]]
        print(f"  {name(s):<20} {swatch(h, k)} {str(tuple(h)):<16} "
              f"{swatch(d, k)} {tuple(d)}")


def rust(schemes, l_hi, l_lo, budget, argv):
    out = [
        "//! The Taipo modifier indicator palette.",
        "//!",
        "//! Generated by `mod-palette.py`; do not edit.  Regenerate with:",
        f"//!     uv run --with numpy python mod-palette.py {' '.join(argv)}",
        "//!",
        "//! Each table is indexed by the bits of `Mods`, so entry 0 is unused.  The values are",
        "//! what goes on the wire: the ws2812's PWM is linear in light, so they are linear, and",
        "//! they are already scaled for the indicator's brightness.",
        "//!",
        f"//! Oklab lightness {l_hi} bright (all held modifiers sticky) and {l_lo} dim (at least",
        f"//! one is one-shot), with the largest channel scaled to {budget}.",
        "",
        "use smart_leds::RGB8;",
        "",
        "/// A modifier indicator palette: one color per modifier set, in two lightnesses.",
        "pub struct ModPalette {",
        "    /// Shown when every held modifier is sticky.",
        "    pub bright: [RGB8; 16],",
        "    /// Shown when at least one held modifier is still one-shot.",
        "    pub dim: [RGB8; 16],",
        "}",
    ]
    for tag, doc, (hi, lo, pwm_hi, pwm_lo, k) in schemes:
        ideal, real, _ = quality(hi, lo, pwm_hi, pwm_lo, k)
        out += ["", f"/// {doc}", "///",
                f"/// Minimum Oklab distance between any two of the 31 states: {ideal:.4f} before",
                f"/// quantization, {real:.4f} after.",
                f"pub static {tag}: ModPalette = ModPalette {{"]
        for field, fam in (("bright", pwm_hi), ("dim", pwm_lo)):
            out.append(f"    {field}: [")
            out.append("        RGB8::new(0, 0, 0),")
            for n, s in enumerate(SETS):
                r, g, b = fam[n]
                out.append(f"        RGB8::new({r:3d}, {g:3d}, {b:3d}), // {name(s)}")
            out.append("    ],")
        out.append("};")
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--l-hi", type=float, default=0.65, help="bright (sticky) Oklab lightness")
    ap.add_argument("--l-lo", type=float, default=0.45, help="dim (one-shot) Oklab lightness")
    ap.add_argument("--budget", type=int, default=32,
                    help="largest 8-bit channel value the indicator may use")
    ap.add_argument("--out", default="src/leds/mod_palette.rs", help="Rust file to write")
    ap.add_argument("--preview", action="store_true", help="also show the palette in the terminal")
    args = ap.parse_args()

    packed = device(assign(packed_palette(args.l_hi)), args.l_hi, args.l_lo, args.budget)
    sums_ab = sums_palette(args.l_hi)
    sums = device({s: sums_ab[n] for n, s in enumerate(SETS)},
                  args.l_hi, args.l_lo, args.budget)

    if args.preview:
        preview("packed", *packed)
        preview("sums", *sums)
        print()

    schemes = [
        ("MOD_PALETTE_PACKED",
         "Packed: the most separable 15 colors, which have to be learned one at a time.", packed),
        ("MOD_PALETTE_SUMS",
         "Sums: four base colors added together, so a combination reads as its parts.", sums),
    ]
    argv = [a for a in sys.argv[1:] if a != "--preview"]
    with open(args.out, "w") as f:
        f.write(rust(schemes, args.l_hi, args.l_lo, args.budget, argv))
    print(f"wrote {args.out}", file=sys.stderr)
    subprocess.run(["rustfmt", args.out], check=False)


if __name__ == "__main__":
    main()
