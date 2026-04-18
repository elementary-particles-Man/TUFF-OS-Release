use crc32fast::Hasher;

use crate::{
    cse_log::CseChunkLogger, kairo_p::PAddressRecord, time, witness::Ring, witness::WitnessRecord,
};

pub struct ClearMini {
    pub ring: Ring,
    chunk_logger: CseChunkLogger,
}

impl ClearMini {
    pub fn new() -> Self {
        time::init_monotonic_base();
        let chunk_logger = CseChunkLogger::new().unwrap_or_else(|e| {
            panic!("failed to initialize CSE chunk logger: {e}");
        });
        Self {
            ring: Ring::new(4096),
            chunk_logger,
        }
    }

    pub fn record(
        &self,
        src: &PAddressRecord,
        dst: &PAddressRecord,
        len: u32,
        flags: u32,
        ip: [u8; 16],
        port: u16,
    ) {
        let mut h = Hasher::new();
        h.update(&ip);
        let r = WitnessRecord {
            mono: time::now_monotonic_ns(),
            utc: time::now_utc_ns(),
            src: src.id,
            dst: dst.id,
            len,
            hash32: h.finalize(),
            flags,
            port,
            ip,
            ..Default::default()
        };
        self.ring.push(r.clone());
        let _ = self.chunk_logger.append_witness(&r);
    }

    /// Return a snapshot of the witness ring for internal control-plane use.
    pub fn dump_witness_snapshot(&self) -> Vec<WitnessRecord> {
        self.ring.snapshot()
    }
}
