-- Sessions the daemon stop (or a reboot) cut off; resumed once at the next start (SPEC §8).
ALTER TABLE session ADD COLUMN interrupted INTEGER NOT NULL DEFAULT 0;
