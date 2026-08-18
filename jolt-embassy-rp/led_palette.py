"""Oklab constant-lightness palette solver.

Taken as-is from the design sketch for the Taipo modifier indicator; see
`mod-palette.py`, which drives it to generate `src/leds/mod_palette.rs`.
The algorithm is deliberately left alone.
"""

import numpy as np

M1 = np.array([
    [0.4122214708, 0.5363325363, 0.0514459929],
    [0.2119034982, 0.6806995451, 0.1073969566],
    [0.0883024619, 0.2817188376, 0.6299787005]])
M2 = np.array([
    [0.2104542553, 0.7936177850, -0.0040720468],
    [1.9779984951, -2.4285922050, 0.4505937099],
    [0.0259040371, 0.7827717662, -0.8086757660]])
M2i = np.linalg.inv(M2)
M1i = np.linalg.inv(M1)


def lin2oklab(rgb):
    rgb = np.atleast_2d(rgb)
    lms = rgb @ M1.T
    lms = np.cbrt(lms)
    return lms @ M2.T


def oklab2lin(lab):
    lab = np.atleast_2d(lab)
    lms = lab @ M2i.T
    lms = lms ** 3
    return lms @ M1i.T


def lin2srgb(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1 / 2.4) - 0.055)


def srgb2lin(c):
    c = np.asarray(c, float)
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def in_gamut(lab, eps=1e-9):
    lin = oklab2lin(lab)
    return np.all((lin >= -eps) & (lin <= 1 + eps), axis=1)


def hexof(lin):
    s = lin2srgb(np.atleast_2d(lin))[0]
    return "#" + "".join(f"{int(round(v * 255)):02X}" for v in s)


def slice_candidates(L, step=0.0035, lim=0.42):
    """In-gamut (a,b) samples on the constant-lightness plane."""
    g = np.arange(-lim, lim + 1e-9, step)
    A, B = np.meshgrid(g, g)
    lab = np.stack([np.full(A.size, L), A.ravel(), B.ravel()], axis=1)
    return lab[in_gamut(lab)]


# ---------------------------------------------------------------------------
# Device primaries.  Replace with your LED's measured CIE xy chromaticities to
# optimise inside the real gamut instead of sRGB.  sRGB values shown as default.
# ---------------------------------------------------------------------------
PRIMARIES_xy = {"r": (0.640, 0.330), "g": (0.300, 0.600), "b": (0.150, 0.060)}
WHITE_xy = (0.3127, 0.3290)


def device_matrix(prim=PRIMARIES_xy, wp=WHITE_xy):
    """RGB->XYZ for a display with the given primaries, normalised to wp."""
    def xyz(xy):
        x, y = xy
        return np.array([x / y, 1.0, (1 - x - y) / y])
    M = np.column_stack([xyz(prim["r"]), xyz(prim["g"]), xyz(prim["b"])])
    S = np.linalg.solve(M, xyz(wp))
    return M * S


def min_pair(P):
    d = np.linalg.norm(P[:, None, :] - P[None, :, :], axis=2)
    np.fill_diagonal(d, np.inf)
    return d.min()


def fit_chroma(ab, L):
    lo, hi = 0.0, 4.0
    for _ in range(50):
        mid = (lo + hi) / 2
        if in_gamut(np.column_stack([np.full(len(ab), L), ab * mid])).all():
            lo = mid
        else:
            hi = mid
    return np.column_stack([np.full(len(ab), L), ab * lo])


def pack(cand, n, restarts=20, iters=80, seed=7):
    rng = np.random.default_rng(seed)
    best, best_s, ab = None, -1.0, cand[:, 1:]
    for _ in range(restarts):
        idx = [int(rng.integers(len(cand)))]
        d = np.linalg.norm(ab - ab[idx[0]], axis=1)
        for _ in range(n - 1):
            j = int(np.argmax(d))
            idx.append(j)
            d = np.minimum(d, np.linalg.norm(ab - ab[j], axis=1))
        idx = np.array(idx)
        for _ in range(iters):
            moved = False
            for k in range(n):
                others = ab[np.delete(idx, k)]
                dd = np.linalg.norm(cand[:, None, 1:] - others[None], axis=2).min(axis=1)
                j = int(np.argmax(dd))
                cur = np.linalg.norm(ab[idx[k]] - others, axis=1).min()
                if dd[j] > cur + 1e-12:
                    idx[k], moved = j, True
            if not moved:
                break
        s = min_pair(cand[idx])
        if s > best_s:
            best_s, best = s, cand[idx].copy()
    return best_s, best


def solve(L_hi=0.65, L_lo=0.34, n=15):
    """Returns (min_dE_over_all_states, bright_lab, dim_lab), hue-ordered."""
    _, P = pack(slice_candidates(L_hi), n)
    ab = P[:, 1:]
    ab = ab[np.argsort(np.arctan2(ab[:, 1], ab[:, 0]))]
    hi, lo = fit_chroma(ab, L_hi), fit_chroma(ab, L_lo)
    return min_pair(np.vstack([hi, lo, np.zeros((1, 3))])), hi, lo


if __name__ == "__main__":
    score, hi, lo = solve()
    print(f"min dE across all {2 * len(hi) + 1} states: {score:.4f}")
    for tag, fam in (("BRIGHT", hi), ("DIM", lo)):
        print(tag, [hexof(oklab2lin(f[None])) for f in fam])
