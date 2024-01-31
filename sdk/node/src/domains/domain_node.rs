use derivative::Derivative;
use sdk_utils::DestructorSet;
use tokio::sync::oneshot;

#[derive(Derivative)]
#[derivative(Debug)]
#[must_use = "Domain node should be closed"]
pub struct DomainNode {
    pub domain_worker_result_receiver: oneshot::Receiver<anyhow::Result<()>>,
    pub _destructors: DestructorSet,
}
