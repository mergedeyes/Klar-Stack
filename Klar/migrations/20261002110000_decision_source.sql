-- Every decision has a source (Art. 17(3)(b) DSA: whether it followed a
-- notice or the team's own initiative). Account measures decided without a
-- report were stored without one, so the decision log's source filter never
-- found them. Those are filled in the way record_account_measure now
-- derives it: an authority's order if one of the reports is, a notice if
-- one came from someone else, otherwise the team's own initiative. Then the
-- column becomes required.
UPDATE moderation_decisions d
SET source = CASE
    WHEN d.rights_claim_id IS NOT NULL THEN 'rights_claim'
    WHEN EXISTS (SELECT 1 FROM reports r WHERE r.id = ANY(d.report_ids) AND r.source = 'authority_order') THEN 'authority_order'
    WHEN EXISTS (SELECT 1 FROM reports r WHERE r.id = ANY(d.report_ids) AND r.source IN ('user_report', 'public_notice')) THEN 'notice'
    ELSE 'own_initiative'
END
WHERE d.source IS NULL;

ALTER TABLE moderation_decisions ALTER COLUMN source SET NOT NULL;
