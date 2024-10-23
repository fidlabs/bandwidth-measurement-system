-- Create serivce_type enum 
CREATE TYPE provider_type AS ENUM ('docker_local', 'aws_fargate');

-- Create services table
CREATE TABLE IF NOT EXISTS services (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    provider_type provider_type NOT NULL,
    is_enabled BOOLEAN NOT NULL,
    details JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Call the trigger function before every update on sub_jobs
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_trigger
        WHERE tgname = 'update_updated_at_trigger'
        AND tgrelid = 'services'::regclass
    ) THEN
        CREATE TRIGGER update_updated_at_trigger
        BEFORE UPDATE ON services
        FOR EACH ROW
        EXECUTE FUNCTION update_updated_at_column();
    END IF;
END $$;

-- Create the service_topics join table
CREATE TABLE  IF NOT EXISTS service_topics (
    service_id UUID NOT NULL,
    topic_id INT NOT NULL,
    PRIMARY KEY (service_id, topic_id),
    FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE CASCADE,
    FOREIGN KEY (topic_id) REFERENCES topics(id) ON DELETE CASCADE
);
