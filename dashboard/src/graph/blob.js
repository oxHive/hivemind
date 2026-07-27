import * as d3 from 'd3'

// Expands a convex hull outward by `pad` along each vertex's normal
// (relative to the hull centroid) so the outline clears the node hexes
// instead of clipping through their centers.
export function padHull(hull, pad) {
  let cx = 0, cy = 0
  for (const [x, y] of hull) { cx += x; cy += y }
  cx /= hull.length
  cy /= hull.length
  return hull.map(([x, y]) => {
    const dx = x - cx, dy = y - cy
    const d = Math.hypot(dx, dy) || 1
    return [x + (dx / d) * pad, y + (dy / d) * pad]
  })
}

// Computes the padded hull (or circle/capsule outline for 1-2 points) that
// `drawClusterBlob` traces around a project's member nodes. `radiusOf(point)`
// returns the node radius at that point's index, mirroring GraphCanvas's
// per-node radius (bigger nodes need more padding to clear their hex).
// Returns `{ kind: 'circle', center, radius } | { kind: 'capsule', a, b, radius } | { kind: 'hull', points }`
// -- geometry only, no canvas drawing -- so it's testable without a browser.
export function clusterOutline(points, radiusOf, pad = 34, hullPad = 30) {
  if (points.length === 0) return null
  const hull = points.length >= 3 ? d3.polygonHull(points) : points
  if (!hull || hull.length === 0) return null

  if (hull.length === 1) {
    return { kind: 'circle', center: hull[0], radius: radiusOf(0) + pad }
  }
  if (hull.length === 2) {
    const [a, b] = hull
    const radius = Math.max(radiusOf(0), radiusOf(1)) + pad
    return { kind: 'capsule', a, b, radius }
  }
  return { kind: 'hull', points: padHull(hull, hullPad) }
}
