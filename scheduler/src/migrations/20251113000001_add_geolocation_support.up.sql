-- Add job_type enum
CREATE TYPE job_type AS ENUM ('bandwidth_saturation', 'geolocation');

-- Add job_type column to jobs table
ALTER TABLE jobs ADD COLUMN job_type job_type NOT NULL DEFAULT 'bandwidth_saturation';

-- Add location column to services table
ALTER TABLE services ADD COLUMN location VARCHAR(255);

-- Add service_id foreign key to workers table
ALTER TABLE workers ADD COLUMN service_id UUID REFERENCES services(id);

-- Create index for location queries
CREATE INDEX idx_services_location ON services(location) WHERE location IS NOT NULL;
CREATE INDEX idx_workers_service_id ON workers(service_id);
