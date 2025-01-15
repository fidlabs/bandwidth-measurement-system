# Bandwith Measurement System (BMS)

The BMS is an experimental system to understand how to estimate available bandwidth capacity of a remote HTTP server under a "non-cooperative" model. Existing systems measuring bandwidth / available capacity between two internet nodes typically involve software running on both systmes. In contrast, BMS attempts to estimate the capacity from one measurement host without any software or coordination from the remote systems being measured. This model leads to several questions:
* What requirements are needed on a remote system-under-test to get a valid measurement (how large of a file should be present on a remote HTTP server? how many vantage points, and with what relationship to the node under test are needed to have confidence in the determination?)
* In what ways could the system be game-able by a node under test that is attempting to manipulate measurments?
* How does confidence increase with additional resource usage? Where are good efficiency points between confidence and cost?
* Is it possible to characterize the bandwidth of remote nodes without impacting their services / acting like a ddos?

The high level design of BMS is to coordinate a set of worker nodes to attempt to download a file from a remote server at the same time. The process is repeated with increasing numbers of measuring nodes until the overall bandwidth observed reaches a plateau / maximum.

## Infrastucture

- Scheduler (Rust)
- Worker (Rust)
- PostgresSQL
- RabbitMQ

## Environment variables

Example env file: [.env.example](./.env.example)

Global ENV:

- `DATABASE_URL`: URI to the PostgresSQL database
- `RABBITMQ_ENDPOINT`: Endpoint of the RabbitMQ server (host:port)
- `RABBITMQ_USERNAME`: Username to authenticate with RabbitMQ
- `RABBITMQ_PASSWORD`: Password to authenticate with RabbitMQ
- `LOG_LEVEL`: Log level of the application (debug, info, warn, error) - default: info

Worker ENV:

- `WORKER_NAME`: Unique identifier of the worker, user to bind to the queue
- `WORKER_TOPICS`: Comma separated list of topics the worker is interested in (All workers are interested in the `all` topic)
- `HEARTBEAT_INTERVAL_SEC` (optional): Interval in seconds between sending heartbeats to the scheduler - default: 5

## Dev Setup

1. `cp .env.example .env` to create the env file
1. `ln -s "$PWD" /tmp/bms` to create symlink to shared storage between the scheduler and the host
1. `docker compose up -d rabbitmq` to start the RabbitMQ (needs to be started before the application)
1. `docker compose up -d` to start the PostgresSQL, scheduler and static worker
1. `WORKER_NAME=worker_manual WORKER_TOPICS=all,europe,poland cargo run --bin worker` to start additional worker

## RabbitMQ Communication

### Job Exchange

![Job Exchange](./docs/bms_queue_job_1.drawio.png)

### Result Exchange

![Result Exchange](./docs/bms_queue_results_1.drawio.png)
