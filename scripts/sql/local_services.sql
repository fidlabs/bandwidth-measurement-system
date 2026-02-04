-- ============================================================================
-- Local Development Services Configuration
-- ============================================================================
-- This file configures the services for local development to match the
-- docker-compose.yml worker services structure.
--
-- Services configured:
--   - worker_eu_pl  (Europe/Poland/Rzeszow)
--   - worker_usa_la (USA/California/Los Angeles)
--   - worker_eu_es  (Europe/Spain/Alicante)
-- ============================================================================

BEGIN;

-- ============================================================================
-- Insert Topics (idempotent)
-- ============================================================================
INSERT INTO topics (name) VALUES
    ('all'),
    ('europe'),
    ('poland'),
    ('rzeszow'),
    ('usa'),
    ('california'),
    ('los_angeles'),
    ('spain'),
    ('alicante')
ON CONFLICT (name) DO NOTHING;

-- ============================================================================
-- Insert Services (idempotent)
-- ============================================================================
-- Note: Using fixed UUIDs for local dev to ensure idempotency
-- ============================================================================

INSERT INTO services (
    id,
    name,
    provider_type,
    is_enabled,
    details
) VALUES
    (
        'a1111111-1111-1111-1111-111111111111'::uuid,
        'worker_eu_pl',
        'docker_local'::provider_type,
        true,
        '{}'::jsonb
    ),
    (
        'a2222222-2222-2222-2222-222222222222'::uuid,
        'worker_usa_la',
        'docker_local'::provider_type,
        true,
        '{}'::jsonb
    ),
    (
        'a3333333-3333-3333-3333-333333333333'::uuid,
        'worker_eu_es',
        'docker_local'::provider_type,
        true,
        '{}'::jsonb
    )
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- Link Services to Topics (idempotent)
-- ============================================================================

-- worker_eu_pl topics: all, europe, poland, rzeszow
INSERT INTO service_topics (service_id, topic_id)
SELECT
    'a1111111-1111-1111-1111-111111111111'::uuid,
    t.id
FROM topics t
WHERE t.name IN ('all', 'europe', 'poland', 'rzeszow')
ON CONFLICT (service_id, topic_id) DO NOTHING;

-- worker_usa_la topics: all, usa, california, los_angeles
INSERT INTO service_topics (service_id, topic_id)
SELECT
    'a2222222-2222-2222-2222-222222222222'::uuid,
    t.id
FROM topics t
WHERE t.name IN ('all', 'usa', 'california', 'los_angeles')
ON CONFLICT (service_id, topic_id) DO NOTHING;

-- worker_eu_es topics: all, europe, spain, alicante
INSERT INTO service_topics (service_id, topic_id)
SELECT
    'a3333333-3333-3333-3333-333333333333'::uuid,
    t.id
FROM topics t
WHERE t.name IN ('all', 'europe', 'spain', 'alicante')
ON CONFLICT (service_id, topic_id) DO NOTHING;

COMMIT;
