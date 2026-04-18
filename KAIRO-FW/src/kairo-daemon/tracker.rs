use crate::matcher::TrafficDirection;
use once_cell::sync::OnceCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::os::fd::RawFd;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct FlowCtx {
    pub pid: u32,
    pub direction: TrafficDirection,
    pub src_port: u16,
    pub dst: Option<SocketAddr>,
    pub bypass: bool,
    pub accumulated_ingress_bytes: u64,
    pub created_at: Instant,
}

#[derive(Default)]
pub struct AgentPidTracker {
    pids: RwLock<HashSet<u32>>,
    flows: RwLock<BTreeMap<RawFd, FlowCtx>>,
    // fallback path while fd-hooks are incomplete
    l4_to_pid: RwLock<HashMap<u16, u32>>,
    bypass_ports: RwLock<HashSet<u16>>,
}

static GLOBAL_TRACKER: OnceCell<Arc<AgentPidTracker>> = OnceCell::new();

impl AgentPidTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_agent_pid(&self, pid: u32) {
        self.pids.write().unwrap().insert(pid);
    }

    pub fn unregister_agent_pid(&self, pid: u32) {
        self.pids.write().unwrap().remove(&pid);
    }

    pub fn is_agent_pid(&self, pid: u32) -> bool {
        self.pids.read().unwrap().contains(&pid)
    }

    pub fn on_socket(&self, fd: RawFd, pid: u32) {
        let ctx = FlowCtx {
            pid,
            direction: TrafficDirection::Egress,
            src_port: 0,
            dst: None,
            bypass: false,
            accumulated_ingress_bytes: 0,
            created_at: Instant::now(),
        };
        self.flows.write().unwrap().insert(fd, ctx);
    }

    pub fn on_connect(&self, fd: RawFd, dst_addr: SocketAddr, src_port: u16) {
        let mut flows = self.flows.write().unwrap();
        let ctx = flows.entry(fd).or_insert_with(|| FlowCtx {
            pid: 0,
            direction: TrafficDirection::Egress,
            src_port,
            dst: Some(dst_addr),
            bypass: false,
            accumulated_ingress_bytes: 0,
            created_at: Instant::now(),
        });
        ctx.direction = TrafficDirection::Egress;
        ctx.dst = Some(dst_addr);
        ctx.src_port = src_port;
    }

    pub fn on_accept(&self, fd: RawFd, pid: u32, remote_addr: SocketAddr, local_port: u16) {
        let ctx = FlowCtx {
            pid,
            direction: TrafficDirection::Ingress,
            src_port: local_port,
            dst: Some(remote_addr),
            bypass: false,
            accumulated_ingress_bytes: 0,
            created_at: Instant::now(),
        };
        self.flows.write().unwrap().insert(fd, ctx);
    }

    pub fn on_recv(&self, fd: RawFd, bytes: usize) -> Option<u64> {
        let mut flows = self.flows.write().unwrap();
        let ctx = flows.get_mut(&fd)?;
        if ctx.direction == TrafficDirection::Ingress {
            ctx.accumulated_ingress_bytes =
                ctx.accumulated_ingress_bytes.saturating_add(bytes as u64);
        }
        Some(ctx.accumulated_ingress_bytes)
    }

    pub fn mark_bypass_fd(&self, fd: RawFd) {
        if let Some(ctx) = self.flows.write().unwrap().get_mut(&fd) {
            ctx.bypass = true;
        }
    }

    pub fn is_bypass_fd(&self, fd: RawFd) -> bool {
        self.flows
            .read()
            .unwrap()
            .get(&fd)
            .map(|x| x.bypass)
            .unwrap_or(false)
    }

    pub fn flow_pid(&self, fd: RawFd) -> Option<u32> {
        self.flows.read().unwrap().get(&fd).map(|x| x.pid)
    }

    pub fn flow_ctx(&self, fd: RawFd) -> Option<FlowCtx> {
        self.flows.read().unwrap().get(&fd).cloned()
    }

    pub fn on_close(&self, fd: RawFd) {
        self.flows.write().unwrap().remove(&fd);
    }

    pub fn map_source_port(&self, source_port: u16, pid: u32) {
        self.l4_to_pid.write().unwrap().insert(source_port, pid);
    }

    pub fn unmap_source_port(&self, source_port: u16) {
        self.l4_to_pid.write().unwrap().remove(&source_port);
        self.bypass_ports.write().unwrap().remove(&source_port);
    }

    pub fn pid_for_port(&self, source_port: u16) -> Option<u32> {
        self.l4_to_pid.read().unwrap().get(&source_port).copied()
    }

    pub fn is_agent_port(&self, source_port: u16) -> bool {
        let Some(pid) = self.pid_for_port(source_port) else {
            return false;
        };
        self.is_agent_pid(pid)
    }

    pub fn mark_bypass_port(&self, source_port: u16) {
        self.bypass_ports.write().unwrap().insert(source_port);
    }

    pub fn is_bypass_port(&self, source_port: u16) -> bool {
        self.bypass_ports.read().unwrap().contains(&source_port)
    }
}

pub fn set_global_tracker(tracker: Arc<AgentPidTracker>) {
    let _ = GLOBAL_TRACKER.set(tracker);
}

pub fn global_tracker() -> Option<Arc<AgentPidTracker>> {
    GLOBAL_TRACKER.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fd_lifecycle_works() {
        let t = AgentPidTracker::new();
        t.register_agent_pid(1234);
        t.on_socket(42, 1234);
        assert_eq!(t.flow_pid(42), Some(1234));
        t.mark_bypass_fd(42);
        assert!(t.is_bypass_fd(42));
        t.on_close(42);
        assert_eq!(t.flow_pid(42), None);
    }

    #[test]
    fn fallback_port_mapping_works() {
        let t = AgentPidTracker::new();
        t.register_agent_pid(4242);
        t.map_source_port(51000, 4242);
        assert!(t.is_agent_port(51000));
        t.mark_bypass_port(51000);
        assert!(t.is_bypass_port(51000));
        t.unmap_source_port(51000);
        assert!(!t.is_bypass_port(51000));
    }
}
