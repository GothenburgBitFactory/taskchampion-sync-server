CREATE TABLE clients (
	client_id UUID PRIMARY KEY,
	latest_version_id UUID default '00000000-0000-0000-0000-000000000000',
	snapshot_version_id UUID,
	versions_since_snapshot INTEGER,
	snapshot_timestamp BIGINT,
	snapshot BYTEA);

CREATE TABLE versions (
	client_id UUID NOT NULL,
	FOREIGN KEY(client_id) REFERENCES clients (client_id) ON DELETE CASCADE,
	version_id UUID NOT NULL,
	parent_version_id UUID,
	history_segment BYTEA,
	CONSTRAINT versions_pkey PRIMARY KEY (client_id, version_id)
);
CREATE INDEX versions_by_parent ON versions (parent_version_id);
