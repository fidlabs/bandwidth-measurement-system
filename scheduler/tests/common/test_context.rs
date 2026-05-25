// scheduler/tests/common/test_context.rs

use std::collections::HashMap;
use std::sync::Arc;

use axum_test::TestServer;
use sqlx::PgPool;

use scheduler::repository::Repositories;
use scheduler::service_repository::ProviderType;
use scheduler::service_scaler::{ServiceScaler, ServiceScalerRegistry};
use scheduler::state::AppState;
use scheduler::url_validator::create_test_acl_client;

use super::{
    get_or_create_containers, ContainerState, MockFileServer, MockServiceScaler, TestDatabase,
};

pub struct TestContext {
    pub db: TestDatabase,
    pub file_server: MockFileServer,
    pub mock_scaler: Arc<MockServiceScaler>,
    pub app: TestServer,
    pub containers: Arc<ContainerState>,
}

impl TestContext {
    pub async fn new() -> Self {
        // 1. Get or create shared containers
        let containers = get_or_create_containers().await;

        // 2. Create isolated database
        let db = TestDatabase::new(containers.postgres_port).await;

        // 3. Start mock file server
        let file_server = MockFileServer::start().await;

        // 4. Create mock scaler
        let mock_scaler = Arc::new(MockServiceScaler::new());

        // 5. Build test application
        let app = Self::create_test_app(&db.pool, mock_scaler.clone()).await;

        Self {
            db,
            file_server,
            mock_scaler,
            app,
            containers,
        }
    }

    async fn create_test_app(pool: &PgPool, mock_scaler: Arc<MockServiceScaler>) -> TestServer {
        // Create repositories
        let repo = Arc::new(Repositories::new(pool.clone()));

        // Create scaler registry with mock
        let mut scalers: HashMap<ProviderType, Arc<dyn ServiceScaler>> = HashMap::new();
        scalers.insert(ProviderType::DockerLocal, mock_scaler);

        let service_scaler_registry = Arc::new(ServiceScalerRegistry::new_with_scalers(scalers));

        // Create ACL client (test version allows localhost)
        let acl_client = create_test_acl_client();

        // Create app state
        let app_state = Arc::new(AppState::new_allowing_private_url_validation(
            repo,
            service_scaler_registry,
            acl_client,
        ));

        // Create router (without swagger UI for tests)
        let app = scheduler::routes::create_routes().with_state(app_state);

        TestServer::new(app).expect("Failed to create test server")
    }

    /// Get database pool for direct queries/seeding
    pub fn pool(&self) -> &PgPool {
        &self.db.pool
    }
}
