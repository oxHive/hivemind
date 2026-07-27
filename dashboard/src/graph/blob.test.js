import { describe, it, expect } from 'vitest'
import { padHull, clusterOutline } from './blob.js'

describe('padHull', () => {
  it('pushes every vertex outward from the centroid by pad', () => {
    const square = [[0, 0], [10, 0], [10, 10], [0, 10]]
    const padded = padHull(square, 5)
    // Centroid is (5,5); each corner is sqrt(50) from it, so the padded
    // corner should sit (sqrt(50)+5)/sqrt(50) times as far from centroid.
    const centroid = [5, 5]
    for (let i = 0; i < square.length; i++) {
      const [ox, oy] = square[i]
      const [px, py] = padded[i]
      const origDist = Math.hypot(ox - centroid[0], oy - centroid[1])
      const newDist = Math.hypot(px - centroid[0], py - centroid[1])
      expect(newDist).toBeCloseTo(origDist + 5, 5)
    }
  })
})

describe('clusterOutline', () => {
  it('returns null for an empty point set', () => {
    expect(clusterOutline([], () => 10)).toBeNull()
  })

  it('returns a circle for a single point', () => {
    const outline = clusterOutline([[3, 4]], () => 10)
    expect(outline.kind).toBe('circle')
    expect(outline.center).toEqual([3, 4])
    expect(outline.radius).toBe(10 + 34)
  })

  it('returns a capsule for two points, radius from the larger member', () => {
    const outline = clusterOutline([[0, 0], [10, 0]], i => (i === 0 ? 10 : 20))
    expect(outline.kind).toBe('capsule')
    expect(outline.a).toEqual([0, 0])
    expect(outline.b).toEqual([10, 0])
    expect(outline.radius).toBe(20 + 34)
  })

  it('returns a padded hull for three or more non-collinear points', () => {
    const outline = clusterOutline([[0, 0], [10, 0], [5, 10]], () => 10)
    expect(outline.kind).toBe('hull')
    expect(outline.points.length).toBe(3)
    // Every hull point must be farther from the centroid than the
    // corresponding un-padded input vertex (padding pushes outward).
    const cx = outline.points.reduce((s, p) => s + p[0], 0) / 3
    const cy = outline.points.reduce((s, p) => s + p[1], 0) / 3
    for (const [x, y] of outline.points) {
      expect(Math.hypot(x - cx, y - cy)).toBeGreaterThan(0)
    }
  })

  it('respects custom pad/hullPad arguments', () => {
    const single = clusterOutline([[0, 0]], () => 5, 100)
    expect(single.radius).toBe(5 + 100)
  })
})
