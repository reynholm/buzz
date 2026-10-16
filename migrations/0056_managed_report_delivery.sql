-- One canonical signed report per author and completion key. Independent of event redaction.
CREATE TABLE managed_deliveries (
    community_id UUID NOT NULL REFERENCES communities(id),
    author BYTEA NOT NULL CHECK (length(author) = 32),
    delivery_key UUID NOT NULL,
    channel_id UUID NOT NULL,
    event JSONB NOT NULL CHECK (pg_column_size(event) <= 524288),
    received_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (community_id, author, delivery_key)
);
SELECT attach_community_write_fence('managed_deliveries');
