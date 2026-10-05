-- SPEC §5, M7. Never edit: schema changes go in a new numbered migration.
ALTER TABLE project ADD COLUMN base_ref TEXT;                          -- Git view base; NULL = auto-detect (origin/HEAD → main → master)
ALTER TABLE doc_comment ADD COLUMN context TEXT NOT NULL DEFAULT 'doc'; -- doc|diff (M7 diff-line comments)
