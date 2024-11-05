-- Add new and change one of sub_jobs status enum values
CREATE TYPE sub_job_status_new AS ENUM ('created', 'pending', 'processing', 'completed', 'failed', 'canceled');
ALTER TABLE sub_jobs ALTER COLUMN status TYPE sub_job_status_new USING status::text::sub_job_status_new;
DROP TYPE sub_job_status;
ALTER TYPE sub_job_status_new RENAME TO sub_job_status;

-- Add new and change one of jobs status enum values
CREATE TYPE job_status_new AS ENUM ('created', 'pending', 'processing', 'completed', 'failed', 'canceled');
ALTER TABLE jobs ALTER COLUMN status TYPE job_status_new USING status::text::job_status_new;
DROP TYPE job_status;
ALTER TYPE job_status_new RENAME TO job_status;

-- Add new sub_jobs type enum value
ALTER TYPE sub_job_type ADD VALUE 'scaling';

-- Add descale_at column to sub_jobs table
ALTER TABLE services ADD COLUMN descale_at TIMESTAMP WITH TIME ZONE;

-- Add deadline_at column to sub_jobs table
ALTER TABLE sub_jobs ADD COLUMN deadline_at TIMESTAMP WITH TIME ZONE;