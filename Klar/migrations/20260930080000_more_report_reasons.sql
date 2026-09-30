-- More report reasons, for violations the first list lumped into "other":
-- scams and fraud, non-consensual intimate images, terrorism and serious
-- threats, trade in illegal goods (drugs, weapons), and extremism
-- (glorifying National Socialism or fascism, supporting extremist
-- organisations of any direction; Terms of Service section 4). See
-- handlers/reports.rs for how each is auto-moderated and moderation.rs for
-- the ground cited.

ALTER TYPE report_reason ADD VALUE IF NOT EXISTS 'fraud';
ALTER TYPE report_reason ADD VALUE IF NOT EXISTS 'ncii';
ALTER TYPE report_reason ADD VALUE IF NOT EXISTS 'terrorism';
ALTER TYPE report_reason ADD VALUE IF NOT EXISTS 'illegal_goods';
ALTER TYPE report_reason ADD VALUE IF NOT EXISTS 'extremism';
