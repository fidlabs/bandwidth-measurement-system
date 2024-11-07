-- Change sub_jobs status enum values
CREATE TYPE sub_job_status_new AS ENUM ('Created', 'Pending', 'Processing', 'Completed', 'Failed', 'Canceled');
ALTER TABLE sub_jobs ALTER COLUMN status TYPE TEXT USING status::text;
UPDATE sub_jobs SET status = initcap(status);
ALTER TABLE sub_jobs ALTER COLUMN status TYPE sub_job_status_new USING status::text::sub_job_status_new;
DROP TYPE sub_job_status;
ALTER TYPE sub_job_status_new RENAME TO sub_job_status;

-- Change sub_jobs type enum values
CREATE TYPE sub_job_type_new AS ENUM ('CombinedDHP', 'Scaling');
ALTER TABLE sub_jobs ALTER COLUMN type TYPE TEXT USING type::text;
UPDATE sub_jobs SET type = 'Scaling' WHERE type = 'scaling';
UPDATE sub_jobs SET type = 'CombinedDHP' WHERE type = 'combineddhp';
ALTER TABLE sub_jobs ALTER COLUMN type TYPE sub_job_type_new USING type::text::sub_job_type_new;
DROP TYPE sub_job_type;
ALTER TYPE sub_job_type_new RENAME TO sub_job_type;

-- Change jobs status enum values
CREATE TYPE job_status_new AS ENUM ('Created', 'Pending', 'Processing', 'Completed', 'Failed', 'Canceled');
ALTER TABLE jobs ALTER COLUMN status TYPE TEXT USING status::text;
UPDATE jobs SET status = initcap(status);
ALTER TABLE jobs ALTER COLUMN status TYPE job_status_new USING status::text::job_status_new;
DROP TYPE job_status;
ALTER TYPE job_status_new RENAME TO job_status;