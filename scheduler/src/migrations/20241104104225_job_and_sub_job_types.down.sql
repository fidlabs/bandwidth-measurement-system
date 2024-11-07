-- Add new and change one of sub_jobs status enum values
CREATE TYPE sub_job_status_new AS ENUM ('created', 'pending', 'processing', 'completed', 'failed', 'canceled');
ALTER TABLE sub_jobs ALTER COLUMN status TYPE TEXT USING status::text;
UPDATE sub_jobs SET status = LOWER(status);
ALTER TABLE sub_jobs ALTER COLUMN status TYPE sub_job_status_new USING status::text::sub_job_status_new;
DROP TYPE sub_job_status;
ALTER TYPE sub_job_status_new RENAME TO sub_job_status;

-- Change sub_jobs type enum values
CREATE TYPE sub_job_type_new AS ENUM ('combineddhp', 'scaling');
ALTER TABLE sub_jobs ALTER COLUMN type TYPE TEXT USING type::text;
UPDATE sub_jobs SET type = 'scaling' WHERE type = 'Scaling';
UPDATE sub_jobs SET type = 'combineddhp' WHERE type = 'CombinedDHP';
ALTER TABLE sub_jobs ALTER COLUMN type TYPE sub_job_type_new USING type::text::sub_job_type_new;
DROP TYPE sub_job_type;
ALTER TYPE sub_job_type_new RENAME TO sub_job_type;

-- Add new and change one of jobs status enum values
CREATE TYPE job_status_new AS ENUM ('created', 'pending', 'processing', 'completed', 'failed', 'canceled');
ALTER TABLE jobs ALTER COLUMN status TYPE TEXT USING status::text;
UPDATE jobs SET status = LOWER(status);
ALTER TABLE jobs ALTER COLUMN status TYPE job_status_new USING status::text::job_status_new;
DROP TYPE job_status;
ALTER TYPE job_status_new RENAME TO job_status;