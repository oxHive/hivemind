-- Reclassify remaining pre-taxonomy relationships into parent/child/sibling.
-- V5 only handled 'mentions'; stores created before the taxonomy still carry
-- auto-linker relationships like shares_tag, applies_to, related_to, pairs_with.
--
-- applies_to: "source applies to target" — the target is the broader
-- context the source falls under, which is exactly the 'parent' semantic.
UPDATE OR IGNORE edges SET relationship = 'parent' WHERE relationship = 'applies_to';

-- Every other legacy relationship is a non-hierarchical association → sibling.
UPDATE OR IGNORE edges SET relationship = 'sibling' WHERE relationship NOT IN ('parent', 'child', 'sibling');

-- OR IGNORE skips rows whose conversion would collide with an existing
-- (source_id, target_id, relationship) edge — e.g. shares_tag and related_to
-- between the same pair both mapping to sibling. Whatever is left is a
-- duplicate of an edge that now exists in the new taxonomy; drop it.
DELETE FROM edges WHERE relationship NOT IN ('parent', 'child', 'sibling');
