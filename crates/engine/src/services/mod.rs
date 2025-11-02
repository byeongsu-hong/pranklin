// Domain services - each service handles a specific domain concern
//
// This follows clean architecture by separating business logic into focused,
// single-responsibility services that orchestrate state changes and event emission.

mod account_service;
mod agent_service;
mod order_service;
mod position_service;

pub use account_service::AccountService;
pub use agent_service::AgentService;
pub use order_service::OrderService;
pub use position_service::PositionService;

use pranklin_types::Event;

/// Service context shared across all domain services
#[derive(Default)]
pub struct ServiceContext {
    /// Event collector for current transaction
    events: Vec<Event>,
}

impl ServiceContext {
    /// Create a new service context
    pub fn new() -> Self {
        Self::default()
    }

    /// Emit a domain event
    pub fn emit(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Take all emitted events
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}
