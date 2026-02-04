-- Drop indexes
DROP INDEX IF EXISTS idx_workers_service_id;
DROP INDEX IF EXISTS idx_services_location;

-- Drop columns
ALTER TABLE workers DROP COLUMN IF EXISTS service_id;
ALTER TABLE services DROP COLUMN IF EXISTS location;
ALTER TABLE jobs DROP COLUMN IF EXISTS job_type;

-- Drop enum
DROP TYPE IF EXISTS job_type;
