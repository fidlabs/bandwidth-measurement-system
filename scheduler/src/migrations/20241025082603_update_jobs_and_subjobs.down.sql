-- Drop the column 
ALTER TABLE services DROP COLUMN descale_at;
ALTER TABLE sub_jobs DROP COLUMN deadline_at;
-- ALTER TABLE jobs DROP COLUMN deadline_at;

-- Recert sub jobs type enum 
DELETE FROM sub_jobs WHERE type = 'scaling';
CREATE TYPE sub_job_type_new AS ENUM ('combineddhp');
ALTER TABLE sub_jobs ALTER COLUMN type TYPE sub_job_type_new USING type::text::sub_job_type_new;
DROP TYPE sub_job_type;
ALTER TYPE sub_job_type_new RENAME TO sub_job_type;
