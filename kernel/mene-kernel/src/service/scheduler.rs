use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Pending,
    Starting,
    Running,
    Failed,
}

pub struct ServiceNode {
    pub id: usize,
    pub name: [u8; 32],
    pub name_len: usize,
    pub deps: Vec<usize>,
    pub state: ServiceState,
}

impl ServiceNode {
    pub fn new(id: usize, name: &[u8]) -> Self {
        let mut name_buf = [0u8; 32];
        let len = name.len().min(32);
        name_buf[..len].copy_from_slice(&name[..len]);
        Self {
            id,
            name: name_buf,
            name_len: len,
            deps: Vec::new(),
            state: ServiceState::Pending,
        }
    }

    pub fn name_bytes(&self) -> &[u8] {
        &self.name[..self.name_len]
    }
}

pub struct DependencyGraph {
    nodes: Vec<ServiceNode>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn add_service(&mut self, name: &[u8]) -> usize {
        let id = self.nodes.len();
        self.nodes.push(ServiceNode::new(id, name));
        id
    }

    pub fn add_dependency(&mut self, service_id: usize, dep_id: usize) {
        if service_id < self.nodes.len() && dep_id < self.nodes.len() {
            self.nodes[service_id].deps.push(dep_id);
        }
    }

    pub fn get_ready_services(&self) -> Vec<usize> {
        let mut ready = Vec::new();
        for node in &self.nodes {
            if node.state == ServiceState::Pending {
                let deps_ready = node.deps.iter().all(|&dep_id| {
                    self.nodes.get(dep_id)
                        .map(|n| n.state == ServiceState::Running)
                        .unwrap_or(false)
                });
                if deps_ready {
                    ready.push(node.id);
                }
            }
        }
        ready
    }

    pub fn mark_starting(&mut self, id: usize) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.state = ServiceState::Starting;
        }
    }

    pub fn mark_running(&mut self, id: usize) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.state = ServiceState::Running;
        }
    }

    pub fn mark_failed(&mut self, id: usize) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.state = ServiceState::Failed;
        }
    }

    pub fn all_completed(&self) -> bool {
        self.nodes.iter().all(|n| {
            n.state == ServiceState::Running || n.state == ServiceState::Failed
        })
    }
}
