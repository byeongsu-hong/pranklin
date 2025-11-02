use super::ServiceContext;
use crate::EngineError;
use pranklin_tx::{Address, RemoveAgentTx, SetAgentTx};
use pranklin_types::Event;

/// Agent service handles agent operations
#[derive(Default)]
pub struct AgentService;

impl AgentService {
    /// Set an agent
    pub fn set_agent(
        &self,
        ctx: &mut ServiceContext,
        account: Address,
        set_agent: &SetAgentTx,
    ) -> Result<(), EngineError> {
        ctx.emit(Event::AgentSet {
            account,
            agent: set_agent.agent,
            permissions: set_agent.permissions,
        });
        Ok(())
    }

    /// Remove an agent
    pub fn remove_agent(
        &self,
        ctx: &mut ServiceContext,
        account: Address,
        remove_agent: &RemoveAgentTx,
    ) -> Result<(), EngineError> {
        ctx.emit(Event::AgentRemoved {
            account,
            agent: remove_agent.agent,
        });
        Ok(())
    }
}
