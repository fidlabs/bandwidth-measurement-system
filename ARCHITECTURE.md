# Bandwidth Measurement System - Architecture Diagrams

This document contains visual architecture diagrams in Mermaid format that can be rendered in Markdown viewers that support Mermaid (GitHub, GitLab, many documentation tools).

## System Overview

```mermaid
graph TB
    subgraph Clients["External Clients"]
        API_Client[API Clients]
        Dashboard[Dashboards]
    end
    
    subgraph Scheduler["Scheduler Service"]
        API[API Layer]
        Background[Background Processes]
        Repo[Repository Layer]
        Scaler[Service Scaler]
    end
    
    subgraph Queue["RabbitMQ"]
        JobExchange[Job Exchange<br/>Topic]
        ResultExchange[Result Exchange<br/>Direct]
        StatusExchange[Status Exchange<br/>Direct]
    end
    
    subgraph Workers["Worker Services"]
        Worker1[Worker EU-PL<br/>Poland]
        Worker2[Worker USA-LA<br/>Los Angeles]
        Worker3[Worker EU-ES<br/>Spain]
        WorkerN[Worker N...]
    end
    
    subgraph Database["PostgreSQL"]
        Jobs[(Jobs)]
        SubJobs[(Sub Jobs)]
        WorkerData[(Worker Data)]
        WorkerStatus[(Worker Status)]
    end
    
    subgraph Targets["Tested Servers"]
        Server1[Server 1]
        Server2[Server 2]
        ServerN[Server N]
    end
    
    Clients -->|HTTP/REST| API
    API --> Repo
    Background --> Repo
    Background --> Scaler
    Background --> JobExchange
    
    JobExchange -->|Route by Topic| Worker1
    JobExchange -->|Route by Topic| Worker2
    JobExchange -->|Route by Topic| Worker3
    JobExchange -->|Route by Topic| WorkerN
    
    Worker1 -->|Test| Server1
    Worker2 -->|Test| Server2
    Worker3 -->|Test| ServerN
    
    Worker1 -->|Results| ResultExchange
    Worker2 -->|Results| ResultExchange
    Worker3 -->|Results| ResultExchange
    WorkerN -->|Results| ResultExchange
    
    Worker1 -->|Status| StatusExchange
    Worker2 -->|Status| StatusExchange
    Worker3 -->|Status| StatusExchange
    
    ResultExchange --> Background
    StatusExchange --> Background
    
    Repo --> Database
    Scaler --> Workers
    
    style Clients fill:#e1f5ff
    style Scheduler fill:#fff4e1
    style Queue fill:#e8f5e9
    style Workers fill:#f3e5f5
    style Database fill:#ffebee
    style Targets fill:#f0f0f0
```

## Job Creation and Processing Flow

```mermaid
sequenceDiagram
    participant Client
    participant Scheduler
    participant DB
    participant SubJobHandler
    participant RabbitMQ
    participant Worker
    participant Server
    
    Client->>Scheduler: POST /jobs
    Scheduler->>DB: Create Job
    Scheduler->>DB: Create Sub Jobs (3)
    Scheduler-->>Client: Return Job ID
    
    Note over SubJobHandler: Background Process (every 5s)
    SubJobHandler->>DB: Get unfinished Sub Job
    
    alt Scaling SubJob
        SubJobHandler->>SubJobHandler: Scale Workers Up
        SubJobHandler->>DB: Update Sub Job Status
    else CombinedDHP SubJob
        SubJobHandler->>DB: Get Online Workers
        SubJobHandler->>SubJobHandler: Calculate Worker Count
        SubJobHandler->>SubJobHandler: Create Job Messages
        SubJobHandler->>RabbitMQ: Publish Job Messages
        SubJobHandler->>DB: Update Status (Pending)
        
        RabbitMQ->>Worker: Deliver Job Message
        Worker->>Worker: Wait for Sync Time
        Worker->>Server: Ping Test
        Worker->>Server: HEAD Request
        Worker->>Server: Download (Range Request)
        Server-->>Worker: Response Data
        
        Worker->>RabbitMQ: Send Results
        RabbitMQ->>Scheduler: Deliver Results
        Scheduler->>DB: Store Results
        Scheduler->>DB: Check Completion
        Scheduler->>DB: Update Sub Job Status
    end
```

## RabbitMQ Message Flow

```mermaid
graph LR
    subgraph Scheduler["Scheduler"]
        JobPublisher[Job Publisher]
        ResultConsumer[Result Consumer]
        StatusConsumer[Status Consumer]
    end
    
    subgraph RabbitMQ["RabbitMQ"]
        JobExchange[Job Exchange<br/>Topic]
        ResultExchange[Result Exchange<br/>Direct]
        StatusExchange[Status Exchange<br/>Direct]
        
        JobQueue1[Queue: all]
        JobQueue2[Queue: europe]
        JobQueue3[Queue: usa]
        ResultQueue[Queue: results]
        StatusQueue[Queue: status]
    end
    
    subgraph Workers["Workers"]
        WorkerAll[Worker<br/>Topic: all]
        WorkerEU[Worker<br/>Topic: all,europe]
        WorkerUSA[Worker<br/>Topic: all,usa]
        ResultPublisher[Result Publisher]
        StatusPublisher[Status Publisher]
    end
    
    JobPublisher -->|Publish| JobExchange
    JobExchange -->|Route| JobQueue1
    JobExchange -->|Route| JobQueue2
    JobExchange -->|Route| JobQueue3
    
    JobQueue1 --> WorkerAll
    JobQueue2 --> WorkerEU
    JobQueue3 --> WorkerUSA
    
    WorkerAll --> ResultPublisher
    WorkerEU --> ResultPublisher
    WorkerUSA --> ResultPublisher
    
    ResultPublisher -->|Publish| ResultExchange
    ResultExchange --> ResultQueue
    ResultQueue --> ResultConsumer
    
    WorkerAll --> StatusPublisher
    WorkerEU --> StatusPublisher
    WorkerUSA --> StatusPublisher
    
    StatusPublisher -->|Publish| StatusExchange
    StatusExchange --> StatusQueue
    StatusQueue --> StatusConsumer
    
    style Scheduler fill:#fff4e1
    style RabbitMQ fill:#e8f5e9
    style Workers fill:#f3e5f5
```

## Worker Execution Flow

```mermaid
flowchart TD
    Start[Worker Receives Job] --> Wait[Wait for Sync Start Time]
    Wait --> Parallel[Execute Tests in Parallel]
    
    Parallel --> Ping[Ping Test]
    Parallel --> Head[HEAD Request Test]
    Parallel --> Download[Download Test]
    
    Ping --> PingResult[Measure Latency<br/>min, max, avg]
    Head --> HeadResult[Measure Response Time<br/>min, max, avg]
    Download --> DownloadResult[Download Chunk<br/>Measure Speed<br/>Track TTFB<br/>Second-by-second logs]
    
    PingResult --> Collect[Collect Results]
    HeadResult --> Collect
    DownloadResult --> Collect
    
    Collect --> Send[Send Result to RabbitMQ]
    Send --> End[End]
    
    style Start fill:#e1f5ff
    style Parallel fill:#fff4e1
    style Collect fill:#e8f5e9
    style End fill:#f3e5f5
```

## Sub Job Processing State Machine

```mermaid
stateDiagram-v2
    [*] --> Created: Sub Job Created
    
    Created --> Pending: Job Messages Published
    Pending --> Processing: Workers Start Processing
    Processing --> Completed: All Workers Complete
    Processing --> Failed: Error Occurred
    
    Created --> Failed: No Workers Available
    Pending --> Failed: Timeout
    
    Completed --> [*]
    Failed --> [*]
    
    note right of Created
        Calculate start times
        Get online workers
        Create job messages
        Publish to RabbitMQ
    end note
    
    note right of Processing
        Workers executing tests
        Results being collected
        Completion checked
    end note
```

## Worker Scaling Flow

```mermaid
sequenceDiagram
    participant SubJobHandler
    participant Scaler
    participant Docker/ECS
    participant Workers
    participant RabbitMQ
    
    SubJobHandler->>SubJobHandler: Detect Scaling SubJob
    SubJobHandler->>Scaler: Scale Up Request
    
    alt Docker Mode
        Scaler->>Docker/ECS: docker compose scale
        Docker/ECS->>Workers: Start Worker Containers
    else Fargate Mode
        Scaler->>Docker/ECS: Update ECS Service Count
        Docker/ECS->>Workers: Launch Fargate Tasks
    end
    
    Workers->>RabbitMQ: Connect & Subscribe
    Workers->>RabbitMQ: Send Online Status
    RabbitMQ->>SubJobHandler: Status Update
    
    Note over SubJobHandler: After Deadline
    SubJobHandler->>Scaler: Scale Down Request
    Scaler->>Docker/ECS: Scale Down
    Docker/ECS->>Workers: Stop Containers/Tasks
    Workers->>RabbitMQ: Send Offline Status
```

## Data Aggregation Flow

```mermaid
flowchart TD
    Start[Job Created] --> CreateSubJobs[Create 3 Sub Jobs]
    
    CreateSubJobs --> SubJob1[SubJob 1: Scaling]
    CreateSubJobs --> SubJob2[SubJob 2: CombinedDHP 80%]
    CreateSubJobs --> SubJob3[SubJob 3: CombinedDHP 100%]
    
    SubJob1 --> Scale[Scale Workers Up]
    Scale --> SubJob2
    
    SubJob2 --> WarmUp[Warm-up Cache<br/>Minimal Workers]
    WarmUp --> Test80[Test with 80% Workers]
    Test80 --> Collect80[Collect Results]
    Collect80 --> SubJob3
    
    SubJob3 --> Test100[Test with 100% Workers]
    Test100 --> Collect100[Collect Results]
    Collect100 --> Aggregate[Aggregate Results]
    
    Aggregate --> Calculate[Calculate Metrics]
    Calculate --> Store[Store in Database]
    Store --> Complete[Job Completed]
    
    style Start fill:#e1f5ff
    style CreateSubJobs fill:#fff4e1
    style Aggregate fill:#e8f5e9
    style Complete fill:#f3e5f5
```

## Component Architecture

```mermaid
graph TB
    subgraph Scheduler["Scheduler"]
        subgraph API["API Layer"]
            JobsAPI[Jobs API]
            ServicesAPI[Services API]
            HealthAPI[Healthcheck API]
        end
        
        subgraph Background["Background"]
            SubJobHandler[Sub Job Handler]
            Descaler[Service Descaler]
            WorkerCheck[Worker Online Check]
        end
        
        subgraph Repo["Repositories"]
            JobRepo[Job Repository]
            SubJobRepo[Sub Job Repository]
            WorkerRepo[Worker Repository]
            DataRepo[Data Repository]
        end
        
        subgraph Scaler["Scaler"]
            DockerScaler[Docker Scaler]
            FargateScaler[Fargate Scaler]
        end
    end
    
    subgraph Worker["Worker"]
        Consumer[Job Consumer]
        DownloadHandler[Download Handler]
        PingHandler[Ping Handler]
        HeadHandler[Head Handler]
        StatusSender[Status Sender]
    end
    
    API --> Repo
    Background --> Repo
    Background --> Scaler
    Background --> RabbitMQ
    
    Worker --> Consumer
    Consumer --> DownloadHandler
    Consumer --> PingHandler
    Consumer --> HeadHandler
    Worker --> StatusSender
    
    style Scheduler fill:#fff4e1
    style Worker fill:#f3e5f5
```

## Database Schema Relationships

```mermaid
erDiagram
    JOBS ||--o{ SUB_JOBS : has
    JOBS ||--o{ WORKER_DATA : contains
    SUB_JOBS ||--o{ WORKER_DATA : contains
    WORKERS ||--o{ WORKER_DATA : generates
    WORKERS ||--o{ WORKER_STATUS : has
    SERVICES ||--o{ WORKERS : manages
    
    JOBS {
        uuid id PK
        string url
        string routing_key
        job_status status
        jsonb details
        timestamp created_at
        timestamp updated_at
    }
    
    SUB_JOBS {
        uuid id PK
        uuid job_id FK
        sub_job_status status
        sub_job_type type
        jsonb details
        int workers_count
        timestamp deadline_at
    }
    
    WORKER_DATA {
        uuid id PK
        uuid run_id
        uuid job_id FK
        uuid sub_job_id FK
        string worker_name
        bool is_success
        jsonb download
        jsonb ping
        jsonb head
    }
    
    WORKERS {
        string name PK
        string[] topics
        worker_status status
        timestamp last_heartbeat
    }
    
    SERVICES {
        uuid id PK
        string name
        provider_type provider_type
        string topic
        int max_instances
        int scale_up_count
        int scale_down_count
        int deadline_minutes
    }
```

## Error Handling Flow

```mermaid
flowchart TD
    Request[API Request] --> Validate{Validate Input}
    Validate -->|Invalid| Error1[Return 400 Bad Request]
    Validate -->|Valid| Process[Process Request]
    
    Process --> DB{Database Operation}
    DB -->|Success| Queue{RabbitMQ Operation}
    DB -->|Error| Error2[Log DB Error]
    
    Queue -->|Success| Return1[Return Success]
    Queue -->|Error| Error3[Log Queue Error]
    
    Error2 --> ErrorHandler[Error Handler]
    Error3 --> ErrorHandler
    
    ErrorHandler --> Log[Log Error]
    ErrorHandler --> Format[Format Error Response]
    Format --> Return2[Return Error Response]
    
    Return1 --> Response[HTTP Response]
    Return2 --> Response
    Error1 --> Response
    
    style Error1 fill:#ffebee
    style Error2 fill:#ffebee
    style Error3 fill:#ffebee
    style ErrorHandler fill:#fff3e0
```

## Worker Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Starting: Worker Starts
    
    Starting --> Connecting: Initialize
    Connecting --> Online: Connected to RabbitMQ
    Connecting --> Failed: Connection Failed
    
    Online --> Subscribed: Subscribe to Topics
    Subscribed --> Idle: Ready
    
    Idle --> Processing: Receive Job
    Processing --> Executing: Execute Tests
    Executing --> Sending: Send Results
    Sending --> Idle: Results Sent
    
    Online --> Heartbeat: Send Heartbeat
    Heartbeat --> Online: Heartbeat Sent
    
    Idle --> Offline: Shutdown Signal
    Processing --> Offline: Shutdown Signal
    Executing --> Offline: Shutdown Signal
    
    Offline --> [*]
    Failed --> [*]
    
    note right of Online
        Send lifecycle status
        Start heartbeat timer
    end note
    
    note right of Processing
        Wait for sync time
        Execute tests
        Collect results
    end note
```

## Deployment Architecture

```mermaid
graph TB
    subgraph Client["Client Layer"]
        Browser[Web Browser]
        API_Client[API Clients]
    end
    
    subgraph LoadBalancer["Load Balancer"]
        LB[NGINX/Cloud LB]
    end
    
    subgraph SchedulerDeploy["Scheduler Deployment"]
        Scheduler1[Scheduler Instance 1]
        Scheduler2[Scheduler Instance 2]
    end
    
    subgraph RabbitMQDeploy["RabbitMQ Cluster"]
        RMQ1[RabbitMQ Node 1]
        RMQ2[RabbitMQ Node 2]
        RMQ3[RabbitMQ Node 3]
    end
    
    subgraph DatabaseDeploy["Database"]
        PostgresPrimary[(PostgreSQL<br/>Primary)]
        PostgresReplica[(PostgreSQL<br/>Replica)]
    end
    
    subgraph WorkerDeploy["Worker Deployment"]
        subgraph Docker["Docker Compose"]
            WorkerEU[EU Workers]
            WorkerUSA[USA Workers]
        end
        subgraph Fargate["AWS ECS Fargate"]
            WorkerFargate1[Fargate Task 1]
            WorkerFargate2[Fargate Task 2]
            WorkerFargateN[Fargate Task N]
        end
    end
    
    Browser --> LB
    API_Client --> LB
    LB --> Scheduler1
    LB --> Scheduler2
    
    Scheduler1 --> RMQ1
    Scheduler2 --> RMQ2
    RMQ1 --> RMQ2
    RMQ2 --> RMQ3
    
    Scheduler1 --> PostgresPrimary
    Scheduler2 --> PostgresPrimary
    Scheduler1 --> PostgresReplica
    
    RMQ1 --> WorkerEU
    RMQ2 --> WorkerUSA
    RMQ3 --> WorkerFargate1
    RMQ3 --> WorkerFargate2
    
    style Client fill:#e1f5ff
    style LoadBalancer fill:#fff4e1
    style SchedulerDeploy fill:#e8f5e9
    style RabbitMQDeploy fill:#f3e5f5
    style DatabaseDeploy fill:#ffebee
    style WorkerDeploy fill:#f0f0f0
```

## Test Execution Timeline

```mermaid
gantt
    title Bandwidth Test Execution Timeline
    dateFormat HH:mm:ss
    axisFormat %H:%M:%S
    
    section Job Creation
    Create Job           :done, create, 00:00:00, 1s
    Create Sub Jobs      :done, subjobs, after create, 1s
    
    section Scaling
    Scale Workers Up    :active, scale, 00:00:01, 30s
    
    section SubJob 1 (80% Workers)
    Warm-up Cache       :warmup, 00:00:31, 10s
    Test Execution      :test80, after warmup, 60s
    Result Collection   :collect80, after test80, 5s
    
    section SubJob 2 (100% Workers)
    Test Execution      :test100, after collect80, 60s
    Result Collection   :collect100, after test100, 5s
    
    section Completion
    Aggregate Results   :agg, after collect100, 5s
    Job Complete        :done, complete, after agg, 1s
```

## Message Types

```mermaid
classDiagram
    class Message {
        <<enumeration>>
        WorkerJob
        WorkerResult
        WorkerStatus
    }
    
    class JobMessage {
        +UUID job_id
        +UUID sub_job_id
        +String url
        +DateTime start_time
        +DateTime download_start_time
        +i64 start_range
        +i64 end_range
        +Vec~String~ excluded_workers
        +i64 log_interval_ms
    }
    
    class ResultMessage {
        +UUID run_id
        +UUID job_id
        +UUID sub_job_id
        +String worker_name
        +bool is_success
        +Result~DownloadResult~ download_result
        +Result~PingResult~ ping_result
        +Result~HeadResult~ head_result
    }
    
    class StatusMessage {
        +String worker_name
        +WorkerStatusDetails status
        +DateTime timestamp
    }
    
    class DownloadResult {
        +usize total_bytes
        +f64 elapsed_secs
        +f64 download_speed
        +DateTime job_start_time
        +DateTime download_start_time
        +DateTime end_time
        +f64 time_to_first_byte_ms
        +Vec second_by_second_logs
    }
    
    Message --> JobMessage : contains
    Message --> ResultMessage : contains
    Message --> StatusMessage : contains
    ResultMessage --> DownloadResult : contains
```
