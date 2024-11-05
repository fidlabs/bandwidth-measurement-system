pub mod data_repository;
pub mod job_repository;
pub mod service_repository;
pub mod sub_job_repository;
pub mod topic_repository;
pub mod worker_repository;

use sqlx::PgPool;

pub use self::data_repository::DataRepository;
pub use self::job_repository::JobRepository;
pub use self::service_repository::ServiceRepository;
pub use self::sub_job_repository::SubJobRepository;
pub use self::topic_repository::TopicRepository;
pub use self::worker_repository::WorkerRepository;

pub struct Repositories {
    pub data: DataRepository,
    pub job: JobRepository,
    pub service: ServiceRepository,
    pub sub_job: SubJobRepository,
    pub topic: TopicRepository,
    pub worker: WorkerRepository,
}

impl Repositories {
    pub fn new(pool: PgPool) -> Self {
        Self {
            data: DataRepository::new(pool.clone()),
            job: JobRepository::new(pool.clone()),
            service: ServiceRepository::new(pool.clone()),
            sub_job: SubJobRepository::new(pool.clone()),
            topic: TopicRepository::new(pool.clone()),
            worker: WorkerRepository::new(pool.clone()),
        }
    }
}
