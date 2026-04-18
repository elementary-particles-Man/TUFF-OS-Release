use std::collections::HashMap;
use std::env;
use std::ffi::{CStr, CString};
use std::future::Future;
use std::hint::black_box;
use std::mem::size_of;
use std::path::Path;
use std::pin::Pin;
use std::slice;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use ash::{vk, Entry};
use parking_lot::Mutex;
use tokio::time::sleep;

const DEFAULT_MIN_BATCH_BYTES: usize = 128 * 1024;
const DEFAULT_PACKET_MIN_BATCH_BYTES: usize = 32 * 1024;
const DEFAULT_MAINTENANCE_MIN_BATCH_BYTES: usize = 64 * 1024;
const DEFAULT_AUDIT_MIN_BATCH_BYTES: usize = 16 * 1024;
const DEFAULT_BULK_MIN_BATCH_BYTES: usize = 64 * 1024;
const DEFAULT_TIMEOUT_MS: u64 = 250;
const DEFAULT_GPU_EXECUTION_MS: u64 = 8;
const DEFAULT_CPU_FALLBACK_MS: u64 = 1;
const MIN_PENDING_POLL_MS: u64 = 1;
const MAX_PENDING_POLL_MS: u64 = 4;
const VULKAN_LOADER_CANDIDATES: [&str; 6] = [
    "/usr/lib/libvulkan.so.1",
    "/usr/lib64/libvulkan.so.1",
    "/usr/lib/x86_64-linux-gnu/libvulkan.so.1",
    "/lib/libvulkan.so.1",
    "/lib64/libvulkan.so.1",
    "/lib/x86_64-linux-gnu/libvulkan.so.1",
];
const RENDER_NODE_CANDIDATES: [&str; 4] = [
    "/dev/dri/renderD128",
    "/dev/dri/renderD129",
    "/dev/dri/card0",
    "/dev/dri/card1",
];

static GLOBAL_BACKEND: once_cell::sync::Lazy<Arc<VulkanBackend>> =
    once_cell::sync::Lazy::new(|| Arc::new(VulkanBackend::new(VulkanBackendConfig::default())));
static PACKET_PREFILTER_SHADER_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/packet_prefilter.comp.spv"));
static AUDIT_PREFILTER_SHADER_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/audit_prefilter.comp.spv"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanBackendState {
    Uninitialized,
    Ready,
    Disabled,
    Faulted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanExecutionPath {
    Vulkan,
    CpuFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanQueueRoutingMode {
    ComputeOnly,
    SplitTransferCompute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanMemoryPath {
    HostVisibleDirect,
    DeviceLocalStaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanWorkloadClass {
    MaintenanceHashing,
    AuditScan,
    PacketPreclassification,
    BulkPrefilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanQueueClass {
    Any,
    ComputeOnly,
    TransferPreferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanPollStatus {
    Pending,
    Completed,
    TimedOut,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanFallbackReason {
    NotInitialized,
    DisabledByPolicy,
    CapabilityUnavailable,
    BelowBatchThreshold,
    Timeout,
    SubmissionRejected,
    DriverUnavailable,
    ProbeStageStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanProbeStage {
    InitOnly,
    AfterResourceAlloc,
    AfterDescriptorUpdate,
    AfterDescriptorUpdateBeforeProbeStageRead,
    AfterProbeStageReadBeforePreFenceBranchEval,
    BeforePreFenceMatchEnter,
    BeforePreFenceMatchEnterWithCleanup,
    BeforePreFenceMatchEnterAfterCleanupBeforeReturn,
    AfterPreFenceMatchEnterBeforeFirstReturnObjectBuild,
    PreFenceMinimalStopReturn,
    PreFenceMinimalStopReturnWithCleanup,
    BeforeCreateFence,
    BeforeFirstPreFenceProbeLog,
    AfterFirstPreFenceProbeLogBeforeReturn,
    BeforeCreateFenceBeforeDescriptorPoolDestroy,
    AfterDescriptorPoolDestroyBeforeFreeCommandBuffers,
    AfterFreeCommandBuffersBeforeDestroyResourceAllocations,
    AfterDestroyResourceAllocationsBeforeProbeStop,
    AfterCreateFenceReturnBeforeProbeStop,
    AfterFenceCreateBeforeCleanup,
    AfterFenceCreate,
    AfterUploadSemaphoreCreate,
    AfterComputeSemaphoreCreate,
    AtRecordFunctionEntryBeforeStepEmit,
    BeforeUploadBeginCommandBuffer,
    AfterUploadBeginCommandBuffer,
    BeforeComputeBeginCommandBuffer,
    AfterComputeBeginCommandBuffer,
    AfterCommandRecordBeforeCleanup,
    AfterCommandRecord,
    AfterQueueSubmit,
    AfterWaitBeforeReadback,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VulkanProbeCleanupMode {
    Cleanup,
    SkipCleanup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VulkanProbePlan {
    stage: VulkanProbeStage,
    skip_cleanup: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VulkanProbeDispatchStop {
    stage: VulkanProbeStage,
    memory_path: Option<VulkanMemoryPath>,
    queue_mode: VulkanQueueRoutingMode,
    uploaded_bytes: u64,
    downloaded_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
struct VulkanGpuCompletion {
    observations: VulkanObservationSet,
    probe_stage: Option<VulkanProbeStage>,
    cleanup_mode: VulkanProbeCleanupMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZeroizeScope {
    DeviceBuffers,
    HostStagingBuffers,
    AllTransientBuffers,
}

#[derive(Debug, Clone)]
pub struct VulkanBackendConfig {
    pub enable_vulkan: bool,
    pub packet_preclassification_min_batch_bytes: usize,
    pub maintenance_hashing_min_batch_bytes: usize,
    pub audit_scan_min_batch_bytes: usize,
    pub bulk_prefilter_min_batch_bytes: usize,
    pub submit_timeout: Duration,
}

impl Default for VulkanBackendConfig {
    fn default() -> Self {
        let global_default = env_usize("KAIRO_VULKAN_MIN_BATCH_BYTES");
        Self {
            enable_vulkan: env_flag("KAIRO_VULKAN_DISABLE")
                .map(|disabled| !disabled)
                .unwrap_or(true),
            packet_preclassification_min_batch_bytes: env_usize(
                "KAIRO_VULKAN_PACKET_MIN_BATCH_BYTES",
            )
            .or(global_default)
            .unwrap_or(DEFAULT_PACKET_MIN_BATCH_BYTES),
            maintenance_hashing_min_batch_bytes: env_usize(
                "KAIRO_VULKAN_MAINTENANCE_MIN_BATCH_BYTES",
            )
            .or(global_default)
            .unwrap_or(DEFAULT_MAINTENANCE_MIN_BATCH_BYTES),
            audit_scan_min_batch_bytes: env_usize("KAIRO_VULKAN_AUDIT_MIN_BATCH_BYTES")
                .or(global_default)
                .unwrap_or(DEFAULT_AUDIT_MIN_BATCH_BYTES),
            bulk_prefilter_min_batch_bytes: env_usize("KAIRO_VULKAN_BULK_MIN_BATCH_BYTES")
                .or(global_default)
                .unwrap_or(DEFAULT_BULK_MIN_BATCH_BYTES),
            submit_timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VulkanBackendCapabilities {
    pub compute_available: bool,
    pub transfer_available: bool,
    pub dedicated_zeroize_supported: bool,
    pub driver_name: String,
    pub device_name: String,
    pub compute_queue_family_index: Option<u32>,
    pub transfer_queue_family_index: Option<u32>,
}

impl Default for VulkanBackendCapabilities {
    fn default() -> Self {
        Self {
            compute_available: false,
            transfer_available: false,
            dedicated_zeroize_supported: false,
            driver_name: "cpu-fallback-contract".to_string(),
            device_name: "unbound".to_string(),
            compute_queue_family_index: None,
            transfer_queue_family_index: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VulkanBatchSubmission {
    pub workload: VulkanWorkloadClass,
    pub queue: VulkanQueueClass,
    pub payload_len: usize,
    pub surface_words: Option<Vec<u32>>,
    pub timeout: Duration,
    pub requires_zeroize: bool,
    pub allows_gpu: bool,
    pub is_boot_or_recovery_path: bool,
    pub is_truth_boundary: bool,
    pub is_single_block_sync: bool,
}

impl VulkanBatchSubmission {
    pub fn maintenance_prefilter(payload_len: usize) -> Self {
        Self {
            workload: VulkanWorkloadClass::BulkPrefilter,
            queue: VulkanQueueClass::Any,
            payload_len,
            surface_words: None,
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            requires_zeroize: false,
            allows_gpu: true,
            is_boot_or_recovery_path: false,
            is_truth_boundary: false,
            is_single_block_sync: false,
        }
    }

    pub fn with_surface_bytes(mut self, bytes: &[u8]) -> Self {
        self.payload_len = bytes.len();
        self.surface_words = Some(pack_surface_words(bytes));
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VulkanBatchHandle {
    id: u64,
}

impl VulkanBatchHandle {
    pub fn id(&self) -> u64 {
        self.id
    }
}

#[derive(Debug, Clone)]
pub struct VulkanBatchResult {
    pub handle: VulkanBatchHandle,
    pub path: VulkanExecutionPath,
    pub workload: VulkanWorkloadClass,
    pub fallback_reason: Option<VulkanFallbackReason>,
    pub probe_stage: Option<VulkanProbeStage>,
    pub completed_at: Instant,
    pub zeroize_required: bool,
    pub queue_mode: VulkanQueueRoutingMode,
    pub memory_path: Option<VulkanMemoryPath>,
    pub packet_observation: Option<VulkanPacketObservation>,
    pub audit_observation: Option<VulkanAuditObservation>,
}

#[derive(Debug, Clone)]
pub struct VulkanPollResult {
    pub status: VulkanPollStatus,
    pub result: Option<VulkanBatchResult>,
    pub suggested_wait: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct VulkanZeroizeRequest {
    pub handle: VulkanBatchHandle,
    pub scope: ZeroizeScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VulkanPacketObservation {
    pub total_bytes: u32,
    pub ascii_bytes: u32,
    pub delimiter_bytes: u32,
    pub non_ascii_bytes: u32,
    pub tech_threat_hits: u32,
    pub harm_threat_hits: u32,
    pub conceal_threat_hits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VulkanAuditObservation {
    pub total_words: u32,
    pub non_zero_words: u32,
    pub high_bit_words: u32,
    pub xor_fold: u32,
}

#[derive(Debug, Clone)]
struct VulkanStoredSubmission {
    workload: VulkanWorkloadClass,
    path: VulkanExecutionPath,
    fallback_reason: Option<VulkanFallbackReason>,
    probe_stage: Option<VulkanProbeStage>,
    zeroize_required: bool,
    zeroize_scope: Option<ZeroizeScope>,
    deadline: Instant,
    ready_at: Instant,
    completed_at: Option<Instant>,
    gpu_submission: Option<VulkanGpuSubmission>,
    queue_mode: VulkanQueueRoutingMode,
    memory_path: Option<VulkanMemoryPath>,
    uploaded_bytes: u64,
    downloaded_bytes: u64,
    packet_observation: Option<VulkanPacketObservation>,
    audit_observation: Option<VulkanAuditObservation>,
}

#[derive(Debug, Clone, Default)]
pub struct VulkanLatencyBuckets {
    pub under_1ms: u64,
    pub under_5ms: u64,
    pub under_20ms: u64,
    pub over_or_equal_20ms: u64,
}

impl VulkanLatencyBuckets {
    pub fn summary(&self) -> String {
        format!(
            "<1ms={} <5ms={} <20ms={} >=20ms={}",
            self.under_1ms, self.under_5ms, self.under_20ms, self.over_or_equal_20ms
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct VulkanWorkloadCounters {
    pub submissions: u64,
    pub vulkan_selected: u64,
    pub vulkan_completed: u64,
    pub cpu_fallbacks: u64,
    pub timeouts: u64,
    pub host_visible_direct: u64,
    pub device_local_staged: u64,
    pub bytes_uploaded: u64,
    pub bytes_downloaded: u64,
    pub latency_buckets: VulkanLatencyBuckets,
}

impl VulkanWorkloadCounters {
    pub fn summary(&self) -> String {
        format!(
            "submissions={} vulkan_selected={} vulkan_completed={} cpu_fallbacks={} timeouts={} host_visible_direct={} device_local_staged={} bytes_up={} bytes_down={} latency={}",
            self.submissions,
            self.vulkan_selected,
            self.vulkan_completed,
            self.cpu_fallbacks,
            self.timeouts,
            self.host_visible_direct,
            self.device_local_staged,
            self.bytes_uploaded,
            self.bytes_downloaded,
            self.latency_buckets.summary()
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct VulkanDebugCounters {
    pub submissions: u64,
    pub vulkan_completions: u64,
    pub cpu_fallbacks: u64,
    pub timeouts: u64,
    pub zeroize_requests: u64,
    pub zeroize_immediate: u64,
    pub bytes_uploaded: u64,
    pub bytes_downloaded: u64,
    pub packet_preclassification: VulkanWorkloadCounters,
    pub maintenance_hashing: VulkanWorkloadCounters,
    pub audit_scan: VulkanWorkloadCounters,
    pub bulk_prefilter: VulkanWorkloadCounters,
}

struct VulkanBackendInner {
    state: VulkanBackendState,
    capabilities: VulkanBackendCapabilities,
    runtime: Option<VulkanPersistentRuntime>,
    next_submission_id: u64,
    submissions: HashMap<u64, VulkanStoredSubmission>,
    counters: VulkanDebugCounters,
}

pub struct VulkanBackend {
    config: VulkanBackendConfig,
    inner: Mutex<VulkanBackendInner>,
}

#[derive(Debug, Clone, Copy)]
struct VulkanBufferAllocation {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    allocation_size: vk::DeviceSize,
    host_visible: bool,
}

#[derive(Debug, Clone, Copy)]
struct VulkanSubmissionResources {
    input: VulkanBufferAllocation,
    output: VulkanBufferAllocation,
    input_staging: Option<VulkanBufferAllocation>,
    output_staging: Option<VulkanBufferAllocation>,
}

#[derive(Debug, Clone, Copy)]
struct VulkanGpuSubmission {
    workload: VulkanWorkloadClass,
    fence: vk::Fence,
    upload_complete_semaphore: Option<vk::Semaphore>,
    compute_complete_semaphore: Option<vk::Semaphore>,
    compute_command_buffer: vk::CommandBuffer,
    transfer_command_buffers: [Option<vk::CommandBuffer>; 2],
    descriptor_pool: vk::DescriptorPool,
    queue_mode: VulkanQueueRoutingMode,
    memory_path: VulkanMemoryPath,
    uploaded_bytes: u64,
    downloaded_bytes: u64,
    resources: VulkanSubmissionResources,
}

struct VulkanPersistentRuntime {
    _entry: ash::Entry,
    instance: ash::Instance,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    compute_queue: vk::Queue,
    transfer_queue: Option<vk::Queue>,
    compute_command_pool: vk::CommandPool,
    transfer_command_pool: Option<vk::CommandPool>,
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    packet_compute_pipeline: vk::Pipeline,
    audit_compute_pipeline: vk::Pipeline,
    compute_queue_family_index: u32,
    transfer_queue_family_index: Option<u32>,
    tuning: VulkanRuntimeTuning,
}

enum GpuSubmissionCompletion {
    Pending,
    Completed(VulkanGpuCompletion),
}

enum VulkanDispatchOutcome {
    Submitted(VulkanGpuSubmission),
    ProbeStopped(VulkanProbeDispatchStop),
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VulkanRecordCommandOutcome {
    Completed,
    ProbeStopped(VulkanProbeStage),
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PrefilterPushConstants {
    total_bytes: u32,
    word_count: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct VulkanObservationSet {
    packet_observation: Option<VulkanPacketObservation>,
    audit_observation: Option<VulkanAuditObservation>,
}

#[derive(Debug, Clone, Copy)]
struct VulkanRuntimeTuning {
    split_transfer_enabled: bool,
    device_local_fast_path: bool,
}

impl VulkanProbeStage {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "init_only" => Some(Self::InitOnly),
            "after_resource_alloc" => Some(Self::AfterResourceAlloc),
            "after_descriptor_update" => Some(Self::AfterDescriptorUpdate),
            "after_descriptor_update_before_probe_stage_read" => {
                Some(Self::AfterDescriptorUpdateBeforeProbeStageRead)
            }
            "after_probe_stage_read_before_prefence_branch_eval" => {
                Some(Self::AfterProbeStageReadBeforePreFenceBranchEval)
            }
            "before_prefence_match_enter" => Some(Self::BeforePreFenceMatchEnter),
            "before_prefence_match_enter_with_cleanup" => {
                Some(Self::BeforePreFenceMatchEnterWithCleanup)
            }
            "before_prefence_match_enter_after_cleanup_before_return" => {
                Some(Self::BeforePreFenceMatchEnterAfterCleanupBeforeReturn)
            }
            "after_prefence_match_enter_before_first_return_object_build" => {
                Some(Self::AfterPreFenceMatchEnterBeforeFirstReturnObjectBuild)
            }
            "prefence_minimal_stop_return" => Some(Self::PreFenceMinimalStopReturn),
            "prefence_minimal_stop_return_with_cleanup" => {
                Some(Self::PreFenceMinimalStopReturnWithCleanup)
            }
            "before_create_fence" => Some(Self::BeforeCreateFence),
            "before_first_prefence_probe_log" => Some(Self::BeforeFirstPreFenceProbeLog),
            "after_first_prefence_probe_log_before_return" => {
                Some(Self::AfterFirstPreFenceProbeLogBeforeReturn)
            }
            "before_create_fence_before_descriptor_pool_destroy" => {
                Some(Self::BeforeCreateFenceBeforeDescriptorPoolDestroy)
            }
            "after_descriptor_pool_destroy_before_free_command_buffers" => {
                Some(Self::AfterDescriptorPoolDestroyBeforeFreeCommandBuffers)
            }
            "after_free_command_buffers_before_destroy_resource_allocations" => {
                Some(Self::AfterFreeCommandBuffersBeforeDestroyResourceAllocations)
            }
            "after_destroy_resource_allocations_before_probe_stop" => {
                Some(Self::AfterDestroyResourceAllocationsBeforeProbeStop)
            }
            "after_create_fence_return_before_probe_stop" => {
                Some(Self::AfterCreateFenceReturnBeforeProbeStop)
            }
            "after_fence_create_before_cleanup" => Some(Self::AfterFenceCreateBeforeCleanup),
            "after_fence_create" => Some(Self::AfterFenceCreate),
            "after_upload_semaphore_create" => Some(Self::AfterUploadSemaphoreCreate),
            "after_compute_semaphore_create" => Some(Self::AfterComputeSemaphoreCreate),
            "at_record_function_entry_before_step_emit" => {
                Some(Self::AtRecordFunctionEntryBeforeStepEmit)
            }
            "before_upload_begin_command_buffer" => Some(Self::BeforeUploadBeginCommandBuffer),
            "after_upload_begin_command_buffer" => Some(Self::AfterUploadBeginCommandBuffer),
            "before_compute_begin_command_buffer" => Some(Self::BeforeComputeBeginCommandBuffer),
            "after_compute_begin_command_buffer" => Some(Self::AfterComputeBeginCommandBuffer),
            "after_command_record_before_cleanup" => Some(Self::AfterCommandRecordBeforeCleanup),
            "after_command_record" => Some(Self::AfterCommandRecord),
            "after_queue_submit" => Some(Self::AfterQueueSubmit),
            "after_wait_before_readback" => Some(Self::AfterWaitBeforeReadback),
            "full" => Some(Self::Full),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::InitOnly => "init_only",
            Self::AfterResourceAlloc => "after_resource_alloc",
            Self::AfterDescriptorUpdate => "after_descriptor_update",
            Self::AfterDescriptorUpdateBeforeProbeStageRead => {
                "after_descriptor_update_before_probe_stage_read"
            }
            Self::AfterProbeStageReadBeforePreFenceBranchEval => {
                "after_probe_stage_read_before_prefence_branch_eval"
            }
            Self::BeforePreFenceMatchEnter => "before_prefence_match_enter",
            Self::BeforePreFenceMatchEnterWithCleanup => "before_prefence_match_enter_with_cleanup",
            Self::BeforePreFenceMatchEnterAfterCleanupBeforeReturn => {
                "before_prefence_match_enter_after_cleanup_before_return"
            }
            Self::AfterPreFenceMatchEnterBeforeFirstReturnObjectBuild => {
                "after_prefence_match_enter_before_first_return_object_build"
            }
            Self::PreFenceMinimalStopReturn => "prefence_minimal_stop_return",
            Self::PreFenceMinimalStopReturnWithCleanup => {
                "prefence_minimal_stop_return_with_cleanup"
            }
            Self::BeforeCreateFence => "before_create_fence",
            Self::BeforeFirstPreFenceProbeLog => "before_first_prefence_probe_log",
            Self::AfterFirstPreFenceProbeLogBeforeReturn => {
                "after_first_prefence_probe_log_before_return"
            }
            Self::BeforeCreateFenceBeforeDescriptorPoolDestroy => {
                "before_create_fence_before_descriptor_pool_destroy"
            }
            Self::AfterDescriptorPoolDestroyBeforeFreeCommandBuffers => {
                "after_descriptor_pool_destroy_before_free_command_buffers"
            }
            Self::AfterFreeCommandBuffersBeforeDestroyResourceAllocations => {
                "after_free_command_buffers_before_destroy_resource_allocations"
            }
            Self::AfterDestroyResourceAllocationsBeforeProbeStop => {
                "after_destroy_resource_allocations_before_probe_stop"
            }
            Self::AfterCreateFenceReturnBeforeProbeStop => {
                "after_create_fence_return_before_probe_stop"
            }
            Self::AfterFenceCreateBeforeCleanup => "after_fence_create_before_cleanup",
            Self::AfterFenceCreate => "after_fence_create",
            Self::AfterUploadSemaphoreCreate => "after_upload_semaphore_create",
            Self::AfterComputeSemaphoreCreate => "after_compute_semaphore_create",
            Self::AtRecordFunctionEntryBeforeStepEmit => {
                "at_record_function_entry_before_step_emit"
            }
            Self::BeforeUploadBeginCommandBuffer => "before_upload_begin_command_buffer",
            Self::AfterUploadBeginCommandBuffer => "after_upload_begin_command_buffer",
            Self::BeforeComputeBeginCommandBuffer => "before_compute_begin_command_buffer",
            Self::AfterComputeBeginCommandBuffer => "after_compute_begin_command_buffer",
            Self::AfterCommandRecordBeforeCleanup => "after_command_record_before_cleanup",
            Self::AfterCommandRecord => "after_command_record",
            Self::AfterQueueSubmit => "after_queue_submit",
            Self::AfterWaitBeforeReadback => "after_wait_before_readback",
            Self::Full => "full",
        }
    }
}

impl VulkanProbePlan {
    fn from_env() -> Option<Self> {
        let stage = env::var("KAIRO_VULKAN_PROBE_STAGE")
            .ok()
            .and_then(|raw| VulkanProbeStage::parse(&raw))?;
        Some(Self {
            stage,
            skip_cleanup: env_flag("KAIRO_VULKAN_PROBE_SKIP_CLEANUP").unwrap_or(false),
        })
    }

    fn cleanup_mode(self) -> VulkanProbeCleanupMode {
        if self.skip_cleanup {
            VulkanProbeCleanupMode::SkipCleanup
        } else {
            VulkanProbeCleanupMode::Cleanup
        }
    }
}

impl VulkanBackend {
    pub fn new(config: VulkanBackendConfig) -> Self {
        Self {
            config,
            inner: Mutex::new(VulkanBackendInner {
                state: VulkanBackendState::Uninitialized,
                capabilities: VulkanBackendCapabilities::default(),
                runtime: None,
                next_submission_id: 1,
                submissions: HashMap::new(),
                counters: VulkanDebugCounters::default(),
            }),
        }
    }

    pub fn initialize(&self) -> VulkanBackendCapabilities {
        let mut inner = self.inner.lock();
        if !self.config.enable_vulkan {
            inner.state = VulkanBackendState::Disabled;
            inner.capabilities = VulkanBackendCapabilities::default();
            inner.runtime = None;
            log::warn!("kairo-daemon: vulkan backend disabled by policy, CPU fallback only");
            return inner.capabilities.clone();
        }

        let (detected, runtime) = initialize_vulkan_runtime();
        inner.state = if runtime.is_some() || detected.transfer_available {
            VulkanBackendState::Ready
        } else {
            VulkanBackendState::Faulted
        };
        inner.capabilities = detected;
        inner.runtime = runtime;
        let queue_mode = inner
            .runtime
            .as_ref()
            .map(|runtime| runtime.queue_routing_mode())
            .unwrap_or(VulkanQueueRoutingMode::ComputeOnly);
        let fast_path = inner
            .runtime
            .as_ref()
            .map(|runtime| runtime.tuning.device_local_fast_path)
            .unwrap_or(false);
        let probe_stage = VulkanProbePlan::from_env().map(|plan| plan.stage.as_str());
        log::info!(
            "kairo-daemon: vulkan backend initialized state={:?} driver={} device={} compute_available={} transfer_available={} compute_qf={:?} transfer_qf={:?} queue_mode={:?} device_local_fast_path={} probe_stage={:?} min_batch_bytes={{packet:{} maintenance:{} audit:{} bulk:{}}}",
            inner.state,
            inner.capabilities.driver_name,
            inner.capabilities.device_name,
            inner.capabilities.compute_available,
            inner.capabilities.transfer_available,
            inner.capabilities.compute_queue_family_index,
            inner.capabilities.transfer_queue_family_index,
            queue_mode,
            fast_path,
            probe_stage,
            self.config.packet_preclassification_min_batch_bytes,
            self.config.maintenance_hashing_min_batch_bytes,
            self.config.audit_scan_min_batch_bytes,
            self.config.bulk_prefilter_min_batch_bytes
        );
        inner.capabilities.clone()
    }

    pub fn state(&self) -> VulkanBackendState {
        self.inner.lock().state
    }

    pub fn capabilities(&self) -> VulkanBackendCapabilities {
        self.inner.lock().capabilities.clone()
    }

    pub fn debug_counters(&self) -> VulkanDebugCounters {
        self.inner.lock().counters.clone()
    }

    pub fn workload_counters(&self, workload: VulkanWorkloadClass) -> VulkanWorkloadCounters {
        let inner = self.inner.lock();
        match workload {
            VulkanWorkloadClass::PacketPreclassification => {
                inner.counters.packet_preclassification.clone()
            }
            VulkanWorkloadClass::MaintenanceHashing => inner.counters.maintenance_hashing.clone(),
            VulkanWorkloadClass::AuditScan => inner.counters.audit_scan.clone(),
            VulkanWorkloadClass::BulkPrefilter => inner.counters.bulk_prefilter.clone(),
        }
    }

    pub fn is_gpu_usable_for(&self, submission: &VulkanBatchSubmission) -> bool {
        let inner = self.inner.lock();
        self.gpu_usable_locked(&inner, submission)
    }

    pub fn submit_batch(&self, submission: VulkanBatchSubmission) -> VulkanBatchHandle {
        let now = Instant::now();
        let timeout = submission.timeout.min(self.config.submit_timeout);
        let mut inner = self.inner.lock();
        let handle = VulkanBatchHandle {
            id: inner.next_submission_id,
        };
        inner.next_submission_id += 1;

        let fallback_reason = self.fallback_reason_locked(&inner, &submission);
        let path = if fallback_reason.is_some() {
            VulkanExecutionPath::CpuFallback
        } else {
            VulkanExecutionPath::Vulkan
        };
        let queue_mode = inner
            .runtime
            .as_ref()
            .map(|runtime| runtime.queue_routing_mode())
            .unwrap_or(VulkanQueueRoutingMode::ComputeOnly);
        let ready_at = now
            + if path == VulkanExecutionPath::Vulkan {
                simulated_gpu_duration(&submission)
            } else {
                simulated_cpu_fallback_duration(&submission)
            };

        inner.submissions.insert(
            handle.id,
            VulkanStoredSubmission {
                workload: submission.workload,
                path,
                fallback_reason,
                probe_stage: None,
                zeroize_required: submission.requires_zeroize,
                zeroize_scope: None,
                deadline: now + timeout,
                ready_at,
                completed_at: None,
                gpu_submission: None,
                queue_mode,
                memory_path: None,
                uploaded_bytes: 0,
                downloaded_bytes: 0,
                packet_observation: None,
                audit_observation: None,
            },
        );
        if path == VulkanExecutionPath::Vulkan {
            let probe_plan = VulkanProbePlan::from_env();
            let dispatch_outcome =
                if probe_plan.map(|plan| plan.stage) == Some(VulkanProbeStage::InitOnly) {
                    None
                } else {
                    Some(
                        inner
                            .runtime
                            .as_ref()
                            .map(|runtime| runtime.dispatch_compute(&submission))
                            .unwrap_or(VulkanDispatchOutcome::Rejected),
                    )
                };
            if let Some(stored) = inner.submissions.get_mut(&handle.id) {
                if probe_plan.map(|plan| plan.stage) == Some(VulkanProbeStage::InitOnly) {
                    stored.path = VulkanExecutionPath::CpuFallback;
                    stored.fallback_reason = Some(VulkanFallbackReason::ProbeStageStop);
                    stored.probe_stage = Some(VulkanProbeStage::InitOnly);
                    stored.completed_at = Some(now);
                    stored.ready_at = now;
                } else {
                    match dispatch_outcome.expect("non-init probe stage has dispatch outcome") {
                        VulkanDispatchOutcome::Submitted(gpu_submission) => {
                            stored.gpu_submission = Some(gpu_submission);
                            stored.ready_at = now + simulated_gpu_duration(&submission);
                            stored.queue_mode = gpu_submission.queue_mode;
                            stored.memory_path = Some(gpu_submission.memory_path);
                            stored.uploaded_bytes = gpu_submission.uploaded_bytes;
                            stored.downloaded_bytes = gpu_submission.downloaded_bytes;
                        }
                        VulkanDispatchOutcome::ProbeStopped(probe) => {
                            stored.path = VulkanExecutionPath::CpuFallback;
                            stored.fallback_reason = Some(VulkanFallbackReason::ProbeStageStop);
                            stored.probe_stage = Some(probe.stage);
                            stored.completed_at = Some(now);
                            stored.ready_at = now;
                            stored.queue_mode = probe.queue_mode;
                            stored.memory_path = probe.memory_path;
                            stored.uploaded_bytes = probe.uploaded_bytes;
                            stored.downloaded_bytes = probe.downloaded_bytes;
                        }
                        VulkanDispatchOutcome::Rejected => {
                            stored.path = VulkanExecutionPath::CpuFallback;
                            stored.fallback_reason = Some(VulkanFallbackReason::SubmissionRejected);
                            stored.ready_at = now + simulated_cpu_fallback_duration(&submission);
                        }
                    }
                }
            }
        }

        let effective = inner
            .submissions
            .get(&handle.id)
            .expect("submission stored");
        let effective_workload = effective.workload;
        let effective_path = effective.path;
        let effective_memory_path = effective.memory_path;
        let effective_uploaded_bytes = effective.uploaded_bytes;
        let effective_downloaded_bytes = effective.downloaded_bytes;
        let effective_queue_mode = effective.queue_mode;
        let effective_fallback_reason = effective.fallback_reason;
        let effective_probe_stage = effective.probe_stage;
        record_submission_counters(
            &mut inner.counters,
            effective_workload,
            effective_path,
            effective_memory_path,
            effective_uploaded_bytes,
            effective_downloaded_bytes,
        );
        match effective_path {
            VulkanExecutionPath::Vulkan => {
                log::info!(
                    "kairo-daemon: accepted async vulkan submission id={} workload={:?} bytes={} queue_mode={:?} memory_path={:?} probe_stage={:?} ready_in_ms={}",
                    handle.id,
                    submission.workload,
                    submission.payload_len,
                    effective_queue_mode,
                    effective_memory_path,
                    effective_probe_stage,
                    ready_at.saturating_duration_since(now).as_millis()
                );
            }
            VulkanExecutionPath::CpuFallback => {
                log::info!(
                    "kairo-daemon: scheduled CPU fallback id={} workload={:?} reason={:?} probe_stage={:?}",
                    handle.id,
                    submission.workload,
                    effective_fallback_reason,
                    effective_probe_stage
                );
            }
        }

        handle
    }

    pub fn poll_completion(&self, handle: VulkanBatchHandle) -> VulkanPollResult {
        let now = Instant::now();
        let mut inner = self.inner.lock();
        let Some(snapshot) = inner.submissions.get(&handle.id).cloned() else {
            return VulkanPollResult {
                status: VulkanPollStatus::Missing,
                result: None,
                suggested_wait: None,
            };
        };

        if let Some(gpu_submission) = snapshot.gpu_submission {
            let completion = inner
                .runtime
                .as_ref()
                .map(|runtime| runtime.completion_status(gpu_submission));
            if let Some(completion) = completion {
                match completion {
                    GpuSubmissionCompletion::Pending => {}
                    GpuSubmissionCompletion::Completed(completed) => {
                        let (
                            workload,
                            path,
                            memory_path,
                            ready_at,
                            zeroize_requested,
                            cleanup_submission,
                        ) = {
                            let stored = inner
                                .submissions
                                .get_mut(&handle.id)
                                .expect("submission stored");
                            stored.completed_at = Some(now);
                            stored.probe_stage = completed.probe_stage;
                            stored.gpu_submission = None;
                            stored.packet_observation = completed.observations.packet_observation;
                            stored.audit_observation = completed.observations.audit_observation;
                            let zeroize_scope = stored.zeroize_scope.take();
                            (
                                stored.workload,
                                stored.path,
                                stored.memory_path,
                                stored.ready_at,
                                zeroize_scope.or(stored
                                    .zeroize_required
                                    .then_some(ZeroizeScope::AllTransientBuffers)),
                                matches!(completed.cleanup_mode, VulkanProbeCleanupMode::Cleanup),
                            )
                        };
                        if cleanup_submission {
                            if let Some(runtime) = inner.runtime.as_ref() {
                                runtime.cleanup_submission(gpu_submission, zeroize_requested);
                            }
                        }
                        record_completion_counters(
                            &mut inner.counters,
                            workload,
                            path,
                            memory_path,
                            now.saturating_duration_since(ready_at),
                        );
                        if let Some(stored) = inner.submissions.get_mut(&handle.id) {
                            stored.completed_at = Some(now);
                        }
                    }
                }
            }
        }

        let Some(state_view) = inner.submissions.get(&handle.id) else {
            return VulkanPollResult {
                status: VulkanPollStatus::Missing,
                result: None,
                suggested_wait: None,
            };
        };

        if now >= state_view.deadline && state_view.completed_at.is_none() {
            let (
                workload,
                memory_path,
                queue_mode,
                packet_observation,
                audit_observation,
                ready_at,
                gpu_submission,
                zeroize_requested,
            ) = {
                let stored = inner
                    .submissions
                    .get_mut(&handle.id)
                    .expect("submission stored");
                let workload = stored.workload;
                let memory_path = stored.memory_path;
                let queue_mode = stored.queue_mode;
                let packet_observation = stored.packet_observation;
                let audit_observation = stored.audit_observation;
                let ready_at = stored.ready_at;
                let gpu_submission = stored.gpu_submission.take();
                let zeroize_scope = stored.zeroize_scope.take();
                let zeroize_requested = zeroize_scope.or(stored
                    .zeroize_required
                    .then_some(ZeroizeScope::AllTransientBuffers));
                stored.completed_at = Some(now);
                stored.path = VulkanExecutionPath::CpuFallback;
                stored.fallback_reason = Some(VulkanFallbackReason::Timeout);
                (
                    workload,
                    memory_path,
                    queue_mode,
                    packet_observation,
                    audit_observation,
                    ready_at,
                    gpu_submission,
                    zeroize_requested,
                )
            };
            if let Some(gpu_submission) = gpu_submission {
                if let Some(runtime) = inner.runtime.as_ref() {
                    runtime.cleanup_submission(gpu_submission, zeroize_requested);
                }
            }
            let latency = now.saturating_duration_since(ready_at);
            record_completion_counters(
                &mut inner.counters,
                workload,
                VulkanExecutionPath::CpuFallback,
                memory_path,
                latency,
            );
            inner.counters.timeouts = inner.counters.timeouts.saturating_add(1);
            let workload_counters = workload_counters_mut(&mut inner.counters, workload);
            workload_counters.timeouts = workload_counters.timeouts.saturating_add(1);
            let timed_out = VulkanBatchResult {
                handle,
                path: VulkanExecutionPath::CpuFallback,
                workload,
                fallback_reason: Some(VulkanFallbackReason::Timeout),
                probe_stage: None,
                completed_at: now,
                zeroize_required: false,
                queue_mode,
                memory_path,
                packet_observation,
                audit_observation,
            };
            return VulkanPollResult {
                status: VulkanPollStatus::TimedOut,
                result: Some(timed_out),
                suggested_wait: None,
            };
        }

        let Some(state_view) = inner.submissions.get(&handle.id) else {
            return VulkanPollResult {
                status: VulkanPollStatus::Missing,
                result: None,
                suggested_wait: None,
            };
        };

        if let Some(completed_at) = state_view.completed_at {
            return VulkanPollResult {
                status: completion_status(state_view),
                result: Some(VulkanBatchResult {
                    handle,
                    path: state_view.path,
                    workload: state_view.workload,
                    fallback_reason: state_view.fallback_reason,
                    probe_stage: state_view.probe_stage,
                    completed_at,
                    zeroize_required: false,
                    queue_mode: state_view.queue_mode,
                    memory_path: state_view.memory_path,
                    packet_observation: state_view.packet_observation,
                    audit_observation: state_view.audit_observation,
                }),
                suggested_wait: None,
            };
        }

        if now < state_view.ready_at {
            return VulkanPollResult {
                status: VulkanPollStatus::Pending,
                result: None,
                suggested_wait: Some(suggested_pending_wait(
                    now,
                    state_view.ready_at,
                    state_view.deadline,
                )),
            };
        }

        let (
            workload,
            path,
            fallback_reason,
            queue_mode,
            memory_path,
            probe_stage,
            packet_observation,
            audit_observation,
            ready_at,
            gpu_submission,
            zeroize_requested,
        ) = {
            let stored = inner
                .submissions
                .get_mut(&handle.id)
                .expect("submission stored");
            stored.completed_at = Some(now);
            let zeroize_scope = stored.zeroize_scope.take();
            (
                stored.workload,
                stored.path,
                stored.fallback_reason,
                stored.queue_mode,
                stored.memory_path,
                stored.probe_stage,
                stored.packet_observation,
                stored.audit_observation,
                stored.ready_at,
                stored.gpu_submission.take(),
                zeroize_scope.or(stored
                    .zeroize_required
                    .then_some(ZeroizeScope::AllTransientBuffers)),
            )
        };
        if let Some(gpu_submission) = gpu_submission {
            if let Some(runtime) = inner.runtime.as_ref() {
                runtime.cleanup_submission(gpu_submission, zeroize_requested);
            }
        }
        record_completion_counters(
            &mut inner.counters,
            workload,
            path,
            memory_path,
            now.saturating_duration_since(ready_at),
        );
        VulkanPollResult {
            status: VulkanPollStatus::Completed,
            result: Some(VulkanBatchResult {
                handle,
                path,
                workload,
                fallback_reason,
                probe_stage,
                completed_at: now,
                zeroize_required: false,
                queue_mode,
                memory_path,
                packet_observation,
                audit_observation,
            }),
            suggested_wait: None,
        }
    }

    pub async fn wait_for_completion(&self, handle: VulkanBatchHandle) -> VulkanBatchResult {
        loop {
            let polled = self.poll_completion(handle);
            match polled.status {
                VulkanPollStatus::Completed | VulkanPollStatus::TimedOut => {
                    let result = polled.result.expect("completed submissions carry result");
                    self.release_submission(handle);
                    return result;
                }
                VulkanPollStatus::Pending => {
                    sleep(
                        polled
                            .suggested_wait
                            .unwrap_or_else(|| Duration::from_millis(MIN_PENDING_POLL_MS)),
                    )
                    .await;
                }
                VulkanPollStatus::Missing => {
                    return VulkanBatchResult {
                        handle,
                        path: VulkanExecutionPath::CpuFallback,
                        workload: VulkanWorkloadClass::BulkPrefilter,
                        fallback_reason: Some(VulkanFallbackReason::SubmissionRejected),
                        probe_stage: None,
                        completed_at: Instant::now(),
                        zeroize_required: false,
                        queue_mode: VulkanQueueRoutingMode::ComputeOnly,
                        memory_path: None,
                        packet_observation: None,
                        audit_observation: None,
                    };
                }
            }
        }
    }

    fn release_submission(&self, handle: VulkanBatchHandle) {
        self.inner.lock().submissions.remove(&handle.id);
    }

    pub fn request_zeroize(&self, request: VulkanZeroizeRequest) -> bool {
        let mut inner = self.inner.lock();
        let immediate_candidate = {
            let Some(stored) = inner.submissions.get_mut(&request.handle.id) else {
                return false;
            };
            stored.zeroize_required = false;
            stored.zeroize_scope = Some(match (stored.zeroize_scope, request.scope) {
                (Some(ZeroizeScope::AllTransientBuffers), _)
                | (_, ZeroizeScope::AllTransientBuffers) => ZeroizeScope::AllTransientBuffers,
                (Some(ZeroizeScope::DeviceBuffers), ZeroizeScope::HostStagingBuffers)
                | (Some(ZeroizeScope::HostStagingBuffers), ZeroizeScope::DeviceBuffers) => {
                    ZeroizeScope::AllTransientBuffers
                }
                (Some(existing), _) => existing,
                (None, scope) => scope,
            });
            (
                stored.gpu_submission,
                stored.completed_at.is_some(),
                stored.zeroize_scope.expect("scope set"),
            )
        };
        inner.counters.zeroize_requests = inner.counters.zeroize_requests.saturating_add(1);
        let (gpu_submission, already_completed, scope) = immediate_candidate;
        let immediate_zeroized = if let (Some(runtime), Some(gpu_submission)) =
            (inner.runtime.as_ref(), gpu_submission)
        {
            if already_completed || runtime.is_submission_complete(gpu_submission) {
                runtime.best_effort_zeroize_submission(gpu_submission, scope)
            } else {
                false
            }
        } else {
            false
        };
        if immediate_zeroized {
            inner.counters.zeroize_immediate = inner.counters.zeroize_immediate.saturating_add(1);
        }
        log::info!(
            "kairo-daemon: zeroize hook acknowledged for submission id={} scope={:?} immediate={}",
            request.handle.id,
            request.scope,
            immediate_zeroized
        );
        true
    }

    fn gpu_usable_locked(
        &self,
        inner: &VulkanBackendInner,
        submission: &VulkanBatchSubmission,
    ) -> bool {
        inner.state == VulkanBackendState::Ready
            && inner.capabilities.compute_available
            && submission.allows_gpu
            && !submission.is_boot_or_recovery_path
            && !submission.is_truth_boundary
            && !submission.is_single_block_sync
            && submission.payload_len >= self.config.min_batch_bytes_for(submission.workload)
    }

    fn fallback_reason_locked(
        &self,
        inner: &VulkanBackendInner,
        submission: &VulkanBatchSubmission,
    ) -> Option<VulkanFallbackReason> {
        if !submission.allows_gpu
            || submission.is_boot_or_recovery_path
            || submission.is_truth_boundary
            || submission.is_single_block_sync
        {
            return Some(VulkanFallbackReason::DisabledByPolicy);
        }
        if inner.state == VulkanBackendState::Disabled {
            return Some(VulkanFallbackReason::DisabledByPolicy);
        }
        if inner.state == VulkanBackendState::Uninitialized {
            return Some(VulkanFallbackReason::NotInitialized);
        }
        if !inner.capabilities.transfer_available {
            return Some(VulkanFallbackReason::DriverUnavailable);
        }
        if !inner.capabilities.compute_available {
            return Some(VulkanFallbackReason::CapabilityUnavailable);
        }
        if submission.payload_len < self.config.min_batch_bytes_for(submission.workload) {
            return Some(VulkanFallbackReason::BelowBatchThreshold);
        }
        None
    }
}

impl VulkanBackendConfig {
    fn min_batch_bytes_for(&self, workload: VulkanWorkloadClass) -> usize {
        match workload {
            VulkanWorkloadClass::PacketPreclassification => {
                self.packet_preclassification_min_batch_bytes
            }
            VulkanWorkloadClass::MaintenanceHashing => self.maintenance_hashing_min_batch_bytes,
            VulkanWorkloadClass::AuditScan => self.audit_scan_min_batch_bytes,
            VulkanWorkloadClass::BulkPrefilter => self.bulk_prefilter_min_batch_bytes,
        }
    }
}

impl VulkanPersistentRuntime {
    fn dispatch_compute(&self, submission: &VulkanBatchSubmission) -> VulkanDispatchOutcome {
        let payload_word = submission.payload_len as u32;
        let input_words = submission
            .surface_words
            .as_deref()
            .unwrap_or(std::slice::from_ref(&payload_word));
        let queue_mode = self.queue_routing_mode();
        log::debug!(
            "kairo-daemon: preparing vulkan workload={:?} shader={} probe_stage={:?}",
            submission.workload,
            workload_shader_label(submission.workload),
            VulkanProbePlan::from_env().map(|plan| plan.stage)
        );
        if self.tuning.device_local_fast_path {
            match self.dispatch_compute_device_local(submission, input_words, queue_mode) {
                VulkanDispatchOutcome::Submitted(gpu_submission) => {
                    log::debug!(
                        "kairo-daemon: vulkan device-local fast path selected workload={:?} queue_mode={:?}",
                        submission.workload,
                        queue_mode
                    );
                    return VulkanDispatchOutcome::Submitted(gpu_submission);
                }
                VulkanDispatchOutcome::ProbeStopped(probe) => {
                    return VulkanDispatchOutcome::ProbeStopped(probe);
                }
                VulkanDispatchOutcome::Rejected => {}
            }
            log::debug!(
                "kairo-daemon: vulkan device-local fast path unavailable, falling back to host-visible direct workload={:?}",
                submission.workload
            );
        }
        self.dispatch_compute_host_visible(submission, input_words, queue_mode)
    }

    fn dispatch_compute_host_visible(
        &self,
        submission: &VulkanBatchSubmission,
        input_words: &[u32],
        queue_mode: VulkanQueueRoutingMode,
    ) -> VulkanDispatchOutcome {
        let output_metrics = [0u32; 7];
        let input = self.create_host_visible_buffer(
            size_of_val_bytes(input_words),
            vk::BufferUsageFlags::STORAGE_BUFFER,
            queue_mode,
        );
        let output = self.create_host_visible_buffer(
            size_of_val_bytes(&output_metrics),
            vk::BufferUsageFlags::STORAGE_BUFFER,
            queue_mode,
        );
        let (Some(input), Some(output)) = (input, output) else {
            return VulkanDispatchOutcome::Rejected;
        };
        if self
            .write_bytes_to_memory(input.memory, u32_slice_as_bytes(input_words))
            .is_none()
            || self
                .write_bytes_to_memory(output.memory, u32_slice_as_bytes(&output_metrics))
                .is_none()
        {
            self.destroy_buffer_allocation(input);
            self.destroy_buffer_allocation(output);
            return VulkanDispatchOutcome::Rejected;
        }
        self.build_and_submit(
            submission,
            input_words.len() as u32,
            VulkanSubmissionResources {
                input,
                output,
                input_staging: None,
                output_staging: None,
            },
            queue_mode,
            VulkanMemoryPath::HostVisibleDirect,
        )
    }

    fn dispatch_compute_device_local(
        &self,
        submission: &VulkanBatchSubmission,
        input_words: &[u32],
        queue_mode: VulkanQueueRoutingMode,
    ) -> VulkanDispatchOutcome {
        let output_metrics = [0u32; 7];
        let input_staging = self.create_host_visible_buffer(
            size_of_val_bytes(input_words),
            vk::BufferUsageFlags::TRANSFER_SRC,
            queue_mode,
        );
        let output_staging = self.create_host_visible_buffer(
            size_of_val_bytes(&output_metrics),
            vk::BufferUsageFlags::TRANSFER_DST,
            queue_mode,
        );
        let input = self.create_device_local_buffer(
            size_of_val_bytes(input_words),
            vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::STORAGE_BUFFER,
            queue_mode,
        );
        let output = self.create_device_local_buffer(
            size_of_val_bytes(&output_metrics),
            vk::BufferUsageFlags::TRANSFER_SRC
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::STORAGE_BUFFER,
            queue_mode,
        );
        let (Some(input_staging), Some(output_staging), Some(input), Some(output)) =
            (input_staging, output_staging, input, output)
        else {
            return VulkanDispatchOutcome::Rejected;
        };
        if self
            .write_bytes_to_memory(input_staging.memory, u32_slice_as_bytes(input_words))
            .is_none()
            || self
                .write_bytes_to_memory(output_staging.memory, u32_slice_as_bytes(&output_metrics))
                .is_none()
        {
            self.destroy_buffer_allocation(input_staging);
            self.destroy_buffer_allocation(output_staging);
            self.destroy_buffer_allocation(input);
            self.destroy_buffer_allocation(output);
            return VulkanDispatchOutcome::Rejected;
        }
        self.build_and_submit(
            submission,
            input_words.len() as u32,
            VulkanSubmissionResources {
                input,
                output,
                input_staging: Some(input_staging),
                output_staging: Some(output_staging),
            },
            queue_mode,
            VulkanMemoryPath::DeviceLocalStaged,
        )
    }

    fn build_and_submit(
        &self,
        submission: &VulkanBatchSubmission,
        word_count: u32,
        resources: VulkanSubmissionResources,
        queue_mode: VulkanQueueRoutingMode,
        memory_path: VulkanMemoryPath,
    ) -> VulkanDispatchOutcome {
        let probe_plan = VulkanProbePlan::from_env();
        let uploaded_bytes = resources
            .input_staging
            .map(|b| b.allocation_size)
            .unwrap_or(resources.input.allocation_size);
        let downloaded_bytes = resources
            .output_staging
            .map(|b| b.allocation_size)
            .unwrap_or(resources.output.allocation_size);
        if probe_plan.map(|plan| plan.stage) == Some(VulkanProbeStage::AfterResourceAlloc) {
            self.destroy_submission_resource_allocations(resources);
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::AfterResourceAlloc.as_str(),
                queue_mode,
                memory_path
            );
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterResourceAlloc,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        let Some(compute_command_buffer) = self.allocate_command_buffer(self.compute_command_pool)
        else {
            self.destroy_submission_resource_allocations(resources);
            return VulkanDispatchOutcome::Rejected;
        };
        let upload_command_buffer = if matches!(memory_path, VulkanMemoryPath::DeviceLocalStaged) {
            match self.allocate_command_buffer(self.transfer_or_compute_command_pool(queue_mode)) {
                Some(command_buffer) => Some(command_buffer),
                None => {
                    self.free_partial_command_buffers(
                        queue_mode,
                        compute_command_buffer,
                        [None, None],
                    );
                    self.destroy_submission_resource_allocations(resources);
                    return VulkanDispatchOutcome::Rejected;
                }
            }
        } else {
            None
        };
        let download_command_buffer = if matches!(memory_path, VulkanMemoryPath::DeviceLocalStaged)
        {
            match self.allocate_command_buffer(self.transfer_or_compute_command_pool(queue_mode)) {
                Some(command_buffer) => Some(command_buffer),
                None => {
                    self.free_partial_command_buffers(
                        queue_mode,
                        compute_command_buffer,
                        [upload_command_buffer, None],
                    );
                    self.destroy_submission_resource_allocations(resources);
                    return VulkanDispatchOutcome::Rejected;
                }
            }
        } else {
            None
        };
        let Some(descriptor_pool) = self.create_descriptor_pool() else {
            self.free_partial_command_buffers(
                queue_mode,
                compute_command_buffer,
                [upload_command_buffer, download_command_buffer],
            );
            self.destroy_submission_resource_allocations(resources);
            return VulkanDispatchOutcome::Rejected;
        };
        let Some(descriptor_set) = self.allocate_descriptor_set(descriptor_pool) else {
            unsafe {
                self.device.destroy_descriptor_pool(descriptor_pool, None);
            }
            self.free_partial_command_buffers(
                queue_mode,
                compute_command_buffer,
                [upload_command_buffer, download_command_buffer],
            );
            self.destroy_submission_resource_allocations(resources);
            return VulkanDispatchOutcome::Rejected;
        };
        self.update_descriptor_set(
            descriptor_set,
            resources.input.buffer,
            resources.output.buffer,
        );
        if probe_plan.map(|plan| plan.stage) == Some(VulkanProbeStage::AfterDescriptorUpdate) {
            unsafe {
                self.device.destroy_descriptor_pool(descriptor_pool, None);
            }
            self.free_partial_command_buffers(
                queue_mode,
                compute_command_buffer,
                [upload_command_buffer, download_command_buffer],
            );
            self.destroy_submission_resource_allocations(resources);
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::AfterDescriptorUpdate.as_str(),
                queue_mode,
                memory_path
            );
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterDescriptorUpdate,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        if probe_plan.map(|plan| plan.stage)
            == Some(VulkanProbeStage::AfterDescriptorUpdateBeforeProbeStageRead)
        {
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterDescriptorUpdateBeforeProbeStageRead,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        let probe_stage = probe_plan.map(|plan| plan.stage);
        if probe_stage == Some(VulkanProbeStage::AfterProbeStageReadBeforePreFenceBranchEval) {
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterProbeStageReadBeforePreFenceBranchEval,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        if probe_stage == Some(VulkanProbeStage::BeforePreFenceMatchEnter) {
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::BeforePreFenceMatchEnter,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        if matches!(
            probe_stage,
            Some(
                VulkanProbeStage::BeforePreFenceMatchEnterWithCleanup
                    | VulkanProbeStage::BeforePreFenceMatchEnterAfterCleanupBeforeReturn
                    | VulkanProbeStage::PreFenceMinimalStopReturnWithCleanup
            )
        ) {
            unsafe {
                self.device.destroy_descriptor_pool(descriptor_pool, None);
            }
            self.free_partial_command_buffers(
                queue_mode,
                compute_command_buffer,
                [upload_command_buffer, download_command_buffer],
            );
            self.destroy_submission_resource_allocations(resources);

            if probe_stage
                == Some(VulkanProbeStage::BeforePreFenceMatchEnterAfterCleanupBeforeReturn)
            {
                return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                    stage: VulkanProbeStage::BeforePreFenceMatchEnterAfterCleanupBeforeReturn,
                    memory_path: Some(memory_path),
                    queue_mode,
                    uploaded_bytes,
                    downloaded_bytes,
                });
            }
            if probe_stage == Some(VulkanProbeStage::PreFenceMinimalStopReturnWithCleanup) {
                return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                    stage: VulkanProbeStage::PreFenceMinimalStopReturnWithCleanup,
                    memory_path: None,
                    queue_mode: VulkanQueueRoutingMode::ComputeOnly,
                    uploaded_bytes: 0,
                    downloaded_bytes: 0,
                });
            }
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::BeforePreFenceMatchEnterWithCleanup,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        if matches!(
            probe_stage,
            Some(
                VulkanProbeStage::AfterPreFenceMatchEnterBeforeFirstReturnObjectBuild
                    | VulkanProbeStage::PreFenceMinimalStopReturn
                    | VulkanProbeStage::BeforeCreateFence
                    | VulkanProbeStage::BeforeFirstPreFenceProbeLog
                    | VulkanProbeStage::AfterFirstPreFenceProbeLogBeforeReturn
                    | VulkanProbeStage::BeforeCreateFenceBeforeDescriptorPoolDestroy
                    | VulkanProbeStage::AfterDescriptorPoolDestroyBeforeFreeCommandBuffers
                    | VulkanProbeStage::AfterFreeCommandBuffersBeforeDestroyResourceAllocations
                    | VulkanProbeStage::AfterDestroyResourceAllocationsBeforeProbeStop
            )
        ) {
            if probe_stage
                == Some(VulkanProbeStage::AfterPreFenceMatchEnterBeforeFirstReturnObjectBuild)
            {
                return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                    stage: VulkanProbeStage::AfterPreFenceMatchEnterBeforeFirstReturnObjectBuild,
                    memory_path: Some(memory_path),
                    queue_mode,
                    uploaded_bytes,
                    downloaded_bytes,
                });
            }
            if probe_stage == Some(VulkanProbeStage::PreFenceMinimalStopReturn) {
                return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                    stage: VulkanProbeStage::PreFenceMinimalStopReturn,
                    memory_path: None,
                    queue_mode: VulkanQueueRoutingMode::ComputeOnly,
                    uploaded_bytes: 0,
                    downloaded_bytes: 0,
                });
            }
            if probe_stage == Some(VulkanProbeStage::BeforeFirstPreFenceProbeLog) {
                return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                    stage: VulkanProbeStage::BeforeFirstPreFenceProbeLog,
                    memory_path: Some(memory_path),
                    queue_mode,
                    uploaded_bytes,
                    downloaded_bytes,
                });
            }
            if probe_stage == Some(VulkanProbeStage::AfterFirstPreFenceProbeLogBeforeReturn) {
                log::info!(
                    "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                    submission.workload,
                    VulkanProbeStage::AfterFirstPreFenceProbeLogBeforeReturn.as_str(),
                    queue_mode,
                    memory_path
                );
                return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                    stage: VulkanProbeStage::AfterFirstPreFenceProbeLogBeforeReturn,
                    memory_path: Some(memory_path),
                    queue_mode,
                    uploaded_bytes,
                    downloaded_bytes,
                });
            }
            if probe_stage == Some(VulkanProbeStage::BeforeCreateFenceBeforeDescriptorPoolDestroy) {
                log::info!(
                    "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                    submission.workload,
                    VulkanProbeStage::BeforeCreateFenceBeforeDescriptorPoolDestroy.as_str(),
                    queue_mode,
                    memory_path
                );
                return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                    stage: VulkanProbeStage::BeforeCreateFenceBeforeDescriptorPoolDestroy,
                    memory_path: Some(memory_path),
                    queue_mode,
                    uploaded_bytes,
                    downloaded_bytes,
                });
            }
            unsafe {
                self.device.destroy_descriptor_pool(descriptor_pool, None);
            }
            if probe_stage
                == Some(VulkanProbeStage::AfterDescriptorPoolDestroyBeforeFreeCommandBuffers)
            {
                log::info!(
                    "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                    submission.workload,
                    VulkanProbeStage::AfterDescriptorPoolDestroyBeforeFreeCommandBuffers
                        .as_str(),
                    queue_mode,
                    memory_path
                );
                return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                    stage: VulkanProbeStage::AfterDescriptorPoolDestroyBeforeFreeCommandBuffers,
                    memory_path: Some(memory_path),
                    queue_mode,
                    uploaded_bytes,
                    downloaded_bytes,
                });
            }
            self.free_partial_command_buffers(
                queue_mode,
                compute_command_buffer,
                [upload_command_buffer, download_command_buffer],
            );
            if probe_stage
                == Some(VulkanProbeStage::AfterFreeCommandBuffersBeforeDestroyResourceAllocations)
            {
                log::info!(
                    "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                    submission.workload,
                    VulkanProbeStage::AfterFreeCommandBuffersBeforeDestroyResourceAllocations
                        .as_str(),
                    queue_mode,
                    memory_path
                );
                return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                    stage:
                        VulkanProbeStage::AfterFreeCommandBuffersBeforeDestroyResourceAllocations,
                    memory_path: Some(memory_path),
                    queue_mode,
                    uploaded_bytes,
                    downloaded_bytes,
                });
            }
            self.destroy_submission_resource_allocations(resources);
            if probe_stage == Some(VulkanProbeStage::AfterDestroyResourceAllocationsBeforeProbeStop)
            {
                log::info!(
                    "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                    submission.workload,
                    VulkanProbeStage::AfterDestroyResourceAllocationsBeforeProbeStop.as_str(),
                    queue_mode,
                    memory_path
                );
                return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                    stage: VulkanProbeStage::AfterDestroyResourceAllocationsBeforeProbeStop,
                    memory_path: Some(memory_path),
                    queue_mode,
                    uploaded_bytes,
                    downloaded_bytes,
                });
            }
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::BeforeCreateFence.as_str(),
                queue_mode,
                memory_path
            );
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::BeforeCreateFence,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        let Some(fence) = (unsafe {
            self.device
                .create_fence(&vk::FenceCreateInfo::builder(), None)
                .ok()
        }) else {
            unsafe {
                self.device.destroy_descriptor_pool(descriptor_pool, None);
            }
            self.free_partial_command_buffers(
                queue_mode,
                compute_command_buffer,
                [upload_command_buffer, download_command_buffer],
            );
            self.destroy_submission_resource_allocations(resources);
            return VulkanDispatchOutcome::Rejected;
        };
        if probe_plan.map(|plan| plan.stage)
            == Some(VulkanProbeStage::AfterCreateFenceReturnBeforeProbeStop)
        {
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::AfterCreateFenceReturnBeforeProbeStop.as_str(),
                queue_mode,
                memory_path
            );
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterCreateFenceReturnBeforeProbeStop,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        if probe_plan.map(|plan| plan.stage)
            == Some(VulkanProbeStage::AfterFenceCreateBeforeCleanup)
        {
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::AfterFenceCreateBeforeCleanup.as_str(),
                queue_mode,
                memory_path
            );
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterFenceCreateBeforeCleanup,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        if probe_plan.map(|plan| plan.stage) == Some(VulkanProbeStage::AfterFenceCreate) {
            let staged = VulkanGpuSubmission {
                workload: submission.workload,
                fence,
                upload_complete_semaphore: None,
                compute_complete_semaphore: None,
                compute_command_buffer,
                transfer_command_buffers: [upload_command_buffer, download_command_buffer],
                descriptor_pool,
                queue_mode,
                memory_path,
                uploaded_bytes,
                downloaded_bytes,
                resources,
            };
            self.destroy_submission_resources(staged);
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::AfterFenceCreate.as_str(),
                queue_mode,
                memory_path
            );
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterFenceCreate,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        let upload_complete_semaphore = if upload_command_buffer.is_some() {
            match self.create_semaphore() {
                Some(semaphore) => Some(semaphore),
                None => {
                    let staged = VulkanGpuSubmission {
                        workload: submission.workload,
                        fence,
                        upload_complete_semaphore: None,
                        compute_complete_semaphore: None,
                        compute_command_buffer,
                        transfer_command_buffers: [upload_command_buffer, download_command_buffer],
                        descriptor_pool,
                        queue_mode,
                        memory_path,
                        uploaded_bytes,
                        downloaded_bytes,
                        resources,
                    };
                    self.destroy_submission_resources(staged);
                    return VulkanDispatchOutcome::Rejected;
                }
            }
        } else {
            None
        };
        if probe_plan.map(|plan| plan.stage) == Some(VulkanProbeStage::AfterUploadSemaphoreCreate) {
            let staged = VulkanGpuSubmission {
                workload: submission.workload,
                fence,
                upload_complete_semaphore,
                compute_complete_semaphore: None,
                compute_command_buffer,
                transfer_command_buffers: [upload_command_buffer, download_command_buffer],
                descriptor_pool,
                queue_mode,
                memory_path,
                uploaded_bytes,
                downloaded_bytes,
                resources,
            };
            self.destroy_submission_resources(staged);
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::AfterUploadSemaphoreCreate.as_str(),
                queue_mode,
                memory_path
            );
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterUploadSemaphoreCreate,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        let compute_complete_semaphore = if download_command_buffer.is_some() {
            match self.create_semaphore() {
                Some(semaphore) => Some(semaphore),
                None => {
                    let staged = VulkanGpuSubmission {
                        workload: submission.workload,
                        fence,
                        upload_complete_semaphore,
                        compute_complete_semaphore: None,
                        compute_command_buffer,
                        transfer_command_buffers: [upload_command_buffer, download_command_buffer],
                        descriptor_pool,
                        queue_mode,
                        memory_path,
                        uploaded_bytes,
                        downloaded_bytes,
                        resources,
                    };
                    self.destroy_submission_resources(staged);
                    return VulkanDispatchOutcome::Rejected;
                }
            }
        } else {
            None
        };
        if probe_plan.map(|plan| plan.stage) == Some(VulkanProbeStage::AfterComputeSemaphoreCreate)
        {
            let staged = VulkanGpuSubmission {
                workload: submission.workload,
                fence,
                upload_complete_semaphore,
                compute_complete_semaphore,
                compute_command_buffer,
                transfer_command_buffers: [upload_command_buffer, download_command_buffer],
                descriptor_pool,
                queue_mode,
                memory_path,
                uploaded_bytes,
                downloaded_bytes,
                resources,
            };
            self.destroy_submission_resources(staged);
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::AfterComputeSemaphoreCreate.as_str(),
                queue_mode,
                memory_path
            );
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterComputeSemaphoreCreate,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        match self.record_command_buffers(
            submission,
            word_count,
            resources,
            compute_command_buffer,
            upload_command_buffer,
            download_command_buffer,
            descriptor_set,
            memory_path,
        ) {
            Some(VulkanRecordCommandOutcome::Completed) => {}
            Some(VulkanRecordCommandOutcome::ProbeStopped(stage)) => {
                log::info!(
                    "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                    submission.workload,
                    stage.as_str(),
                    queue_mode,
                    memory_path
                );
                return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                    stage,
                    memory_path: Some(memory_path),
                    queue_mode,
                    uploaded_bytes,
                    downloaded_bytes,
                });
            }
            None => {
                let staged = VulkanGpuSubmission {
                    workload: submission.workload,
                    fence,
                    upload_complete_semaphore,
                    compute_complete_semaphore,
                    compute_command_buffer,
                    transfer_command_buffers: [upload_command_buffer, download_command_buffer],
                    descriptor_pool,
                    queue_mode,
                    memory_path,
                    uploaded_bytes: resources
                        .input_staging
                        .map(|b| b.allocation_size)
                        .unwrap_or(resources.input.allocation_size),
                    downloaded_bytes: resources
                        .output_staging
                        .map(|b| b.allocation_size)
                        .unwrap_or(resources.output.allocation_size),
                    resources,
                };
                self.destroy_submission_resources(staged);
                return VulkanDispatchOutcome::Rejected;
            }
        }
        if probe_plan.map(|plan| plan.stage)
            == Some(VulkanProbeStage::AfterCommandRecordBeforeCleanup)
        {
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::AfterCommandRecordBeforeCleanup.as_str(),
                queue_mode,
                memory_path
            );
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterCommandRecordBeforeCleanup,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        if probe_plan.map(|plan| plan.stage) == Some(VulkanProbeStage::AfterCommandRecord) {
            let staged = VulkanGpuSubmission {
                workload: submission.workload,
                fence,
                upload_complete_semaphore,
                compute_complete_semaphore,
                compute_command_buffer,
                transfer_command_buffers: [upload_command_buffer, download_command_buffer],
                descriptor_pool,
                queue_mode,
                memory_path,
                uploaded_bytes,
                downloaded_bytes,
                resources,
            };
            self.destroy_submission_resources(staged);
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::AfterCommandRecord.as_str(),
                queue_mode,
                memory_path
            );
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterCommandRecord,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        let gpu_submission = VulkanGpuSubmission {
            workload: submission.workload,
            fence,
            upload_complete_semaphore,
            compute_complete_semaphore,
            compute_command_buffer,
            transfer_command_buffers: [upload_command_buffer, download_command_buffer],
            descriptor_pool,
            queue_mode,
            memory_path,
            uploaded_bytes,
            downloaded_bytes,
            resources,
        };
        if self.submit_recorded_commands(gpu_submission).is_none() {
            self.destroy_submission_resources(gpu_submission);
            return VulkanDispatchOutcome::Rejected;
        }
        if probe_plan.map(|plan| plan.stage) == Some(VulkanProbeStage::AfterQueueSubmit) {
            self.destroy_submission_resources(gpu_submission);
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::AfterQueueSubmit.as_str(),
                queue_mode,
                memory_path
            );
            return VulkanDispatchOutcome::ProbeStopped(VulkanProbeDispatchStop {
                stage: VulkanProbeStage::AfterQueueSubmit,
                memory_path: Some(memory_path),
                queue_mode,
                uploaded_bytes,
                downloaded_bytes,
            });
        }
        VulkanDispatchOutcome::Submitted(gpu_submission)
    }

    #[allow(clippy::too_many_arguments)]
    fn record_command_buffers(
        &self,
        submission: &VulkanBatchSubmission,
        word_count: u32,
        resources: VulkanSubmissionResources,
        compute_command_buffer: vk::CommandBuffer,
        upload_command_buffer: Option<vk::CommandBuffer>,
        download_command_buffer: Option<vk::CommandBuffer>,
        descriptor_set: vk::DescriptorSet,
        memory_path: VulkanMemoryPath,
    ) -> Option<VulkanRecordCommandOutcome> {
        let probe_stage = VulkanProbePlan::from_env().map(|plan| plan.stage);
        if probe_stage == Some(VulkanProbeStage::AtRecordFunctionEntryBeforeStepEmit) {
            return Some(VulkanRecordCommandOutcome::ProbeStopped(
                VulkanProbeStage::AtRecordFunctionEntryBeforeStepEmit,
            ));
        }
        let emit_record_step = |step: &str| {
            if probe_stage.is_some() {
                eprintln!(
                    "probe_record_step workload={:?} stage={:?} memory_path={:?} step={}",
                    submission.workload, probe_stage, memory_path, step
                );
            }
        };
        let should_stop = |stage| probe_stage == Some(stage);
        let begin_info = vk::CommandBufferBeginInfo::builder();
        unsafe {
            if let Some(upload) = upload_command_buffer {
                if should_stop(VulkanProbeStage::BeforeUploadBeginCommandBuffer) {
                    return Some(VulkanRecordCommandOutcome::ProbeStopped(
                        VulkanProbeStage::BeforeUploadBeginCommandBuffer,
                    ));
                }
                emit_record_step("upload_begin_command_buffer");
                self.device.begin_command_buffer(upload, &begin_info).ok()?;
                if should_stop(VulkanProbeStage::AfterUploadBeginCommandBuffer) {
                    return Some(VulkanRecordCommandOutcome::ProbeStopped(
                        VulkanProbeStage::AfterUploadBeginCommandBuffer,
                    ));
                }
                let copy_region = [vk::BufferCopy::builder()
                    .size(resources.input_staging?.allocation_size)
                    .build()];
                emit_record_step("upload_copy_input");
                self.device.cmd_copy_buffer(
                    upload,
                    resources.input_staging?.buffer,
                    resources.input.buffer,
                    &copy_region,
                );
                emit_record_step("upload_fill_output");
                self.device.cmd_fill_buffer(
                    upload,
                    resources.output.buffer,
                    0,
                    resources.output.allocation_size,
                    0,
                );
                emit_record_step("upload_end_command_buffer");
                self.device.end_command_buffer(upload).ok()?;
            }

            if should_stop(VulkanProbeStage::BeforeComputeBeginCommandBuffer) {
                return Some(VulkanRecordCommandOutcome::ProbeStopped(
                    VulkanProbeStage::BeforeComputeBeginCommandBuffer,
                ));
            }
            emit_record_step("compute_begin_command_buffer");
            self.device
                .begin_command_buffer(compute_command_buffer, &begin_info)
                .ok()?;
            if should_stop(VulkanProbeStage::AfterComputeBeginCommandBuffer) {
                return Some(VulkanRecordCommandOutcome::ProbeStopped(
                    VulkanProbeStage::AfterComputeBeginCommandBuffer,
                ));
            }
            if matches!(memory_path, VulkanMemoryPath::DeviceLocalStaged) {
                let barrier = [vk::BufferMemoryBarrier::builder()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .buffer(resources.input.buffer)
                    .offset(0)
                    .size(resources.input.allocation_size)
                    .build()];
                emit_record_step("compute_barrier_transfer_to_shader");
                self.device.cmd_pipeline_barrier(
                    compute_command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &barrier,
                    &[],
                );
            } else {
                let barrier = [
                    vk::BufferMemoryBarrier::builder()
                        .src_access_mask(vk::AccessFlags::HOST_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .buffer(resources.input.buffer)
                        .offset(0)
                        .size(resources.input.allocation_size)
                        .build(),
                    vk::BufferMemoryBarrier::builder()
                        .src_access_mask(vk::AccessFlags::HOST_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                        .buffer(resources.output.buffer)
                        .offset(0)
                        .size(resources.output.allocation_size)
                        .build(),
                ];
                emit_record_step("compute_barrier_host_to_shader");
                self.device.cmd_pipeline_barrier(
                    compute_command_buffer,
                    vk::PipelineStageFlags::HOST,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &barrier,
                    &[],
                );
            }
            emit_record_step("compute_bind_pipeline");
            self.device.cmd_bind_pipeline(
                compute_command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_for_workload(submission.workload),
            );
            emit_record_step("compute_bind_descriptor_sets");
            self.device.cmd_bind_descriptor_sets(
                compute_command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[descriptor_set],
                &[],
            );
            let push_constants = PrefilterPushConstants {
                total_bytes: submission.payload_len as u32,
                word_count,
            };
            emit_record_step("compute_push_constants");
            self.device.cmd_push_constants(
                compute_command_buffer,
                self.pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                any_as_bytes(&push_constants),
            );
            emit_record_step("compute_dispatch");
            self.device.cmd_dispatch(
                compute_command_buffer,
                dispatch_group_count(submission),
                1,
                1,
            );
            if download_command_buffer.is_none() {
                let barrier = [vk::BufferMemoryBarrier::builder()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ)
                    .buffer(resources.output.buffer)
                    .offset(0)
                    .size(resources.output.allocation_size)
                    .build()];
                emit_record_step("compute_barrier_shader_to_host");
                self.device.cmd_pipeline_barrier(
                    compute_command_buffer,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::HOST,
                    vk::DependencyFlags::empty(),
                    &[],
                    &barrier,
                    &[],
                );
            }
            emit_record_step("compute_end_command_buffer");
            self.device
                .end_command_buffer(compute_command_buffer)
                .ok()?;

            if let Some(download) = download_command_buffer {
                emit_record_step("download_begin_command_buffer");
                self.device
                    .begin_command_buffer(download, &begin_info)
                    .ok()?;
                let barrier = [vk::BufferMemoryBarrier::builder()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .buffer(resources.output.buffer)
                    .offset(0)
                    .size(resources.output.allocation_size)
                    .build()];
                emit_record_step("download_barrier_shader_to_transfer");
                self.device.cmd_pipeline_barrier(
                    download,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &barrier,
                    &[],
                );
                let copy_region = [vk::BufferCopy::builder()
                    .size(resources.output_staging?.allocation_size)
                    .build()];
                emit_record_step("download_copy_output");
                self.device.cmd_copy_buffer(
                    download,
                    resources.output.buffer,
                    resources.output_staging?.buffer,
                    &copy_region,
                );
                emit_record_step("download_end_command_buffer");
                self.device.end_command_buffer(download).ok()?;
            }
        }
        Some(VulkanRecordCommandOutcome::Completed)
    }

    fn submit_recorded_commands(&self, submission: VulkanGpuSubmission) -> Option<()> {
        unsafe {
            if let Some(upload) = submission.transfer_command_buffers[0] {
                let signal = [submission.upload_complete_semaphore?];
                let upload_buffers = [upload];
                let submit = [vk::SubmitInfo::builder()
                    .command_buffers(&upload_buffers)
                    .signal_semaphores(&signal)
                    .build()];
                self.device
                    .queue_submit(
                        self.transfer_or_compute_queue(submission.queue_mode),
                        &submit,
                        vk::Fence::null(),
                    )
                    .ok()?;
            }

            let compute_buffers = [submission.compute_command_buffer];
            let mut compute_submit = vk::SubmitInfo::builder().command_buffers(&compute_buffers);
            let wait_stage = [vk::PipelineStageFlags::COMPUTE_SHADER];
            let wait_upload = submission
                .upload_complete_semaphore
                .map(|semaphore| [semaphore]);
            if let Some(ref wait_semaphores) = wait_upload {
                compute_submit = compute_submit
                    .wait_semaphores(wait_semaphores)
                    .wait_dst_stage_mask(&wait_stage);
            }
            let signal_compute = submission
                .compute_complete_semaphore
                .map(|semaphore| [semaphore]);
            if let Some(ref signal_semaphores) = signal_compute {
                compute_submit = compute_submit.signal_semaphores(signal_semaphores);
            }
            let compute_submit = [compute_submit.build()];
            self.device
                .queue_submit(
                    self.compute_queue,
                    &compute_submit,
                    if submission.transfer_command_buffers[1].is_some() {
                        vk::Fence::null()
                    } else {
                        submission.fence
                    },
                )
                .ok()?;

            if let Some(download) = submission.transfer_command_buffers[1] {
                let wait = [submission.compute_complete_semaphore?];
                let wait_stage = [vk::PipelineStageFlags::TRANSFER];
                let download_buffers = [download];
                let download_submit = [vk::SubmitInfo::builder()
                    .command_buffers(&download_buffers)
                    .wait_semaphores(&wait)
                    .wait_dst_stage_mask(&wait_stage)
                    .build()];
                self.device
                    .queue_submit(
                        self.transfer_or_compute_queue(submission.queue_mode),
                        &download_submit,
                        submission.fence,
                    )
                    .ok()?;
            }
        }
        Some(())
    }

    fn allocate_command_buffer(&self, command_pool: vk::CommandPool) -> Option<vk::CommandBuffer> {
        let command_buffer_allocate = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        unsafe {
            self.device
                .allocate_command_buffers(&command_buffer_allocate)
                .ok()?
                .into_iter()
                .next()
        }
    }

    fn create_descriptor_pool(&self) -> Option<vk::DescriptorPool> {
        let descriptor_pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 2,
        }];
        let descriptor_pool_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&descriptor_pool_sizes)
            .max_sets(1);
        unsafe {
            self.device
                .create_descriptor_pool(&descriptor_pool_info, None)
                .ok()
        }
    }

    fn allocate_descriptor_set(
        &self,
        descriptor_pool: vk::DescriptorPool,
    ) -> Option<vk::DescriptorSet> {
        let descriptor_set_layouts = [self.descriptor_set_layout];
        let descriptor_set_allocate = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&descriptor_set_layouts);
        unsafe {
            self.device
                .allocate_descriptor_sets(&descriptor_set_allocate)
                .ok()?
                .into_iter()
                .next()
        }
    }

    fn update_descriptor_set(
        &self,
        descriptor_set: vk::DescriptorSet,
        input_buffer: vk::Buffer,
        output_buffer: vk::Buffer,
    ) {
        let input_buffer_info = [vk::DescriptorBufferInfo {
            buffer: input_buffer,
            offset: 0,
            range: vk::WHOLE_SIZE,
        }];
        let output_buffer_info = [vk::DescriptorBufferInfo {
            buffer: output_buffer,
            offset: 0,
            range: vk::WHOLE_SIZE,
        }];
        let descriptor_writes = [
            vk::WriteDescriptorSet::builder()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&input_buffer_info)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&output_buffer_info)
                .build(),
        ];
        unsafe {
            self.device.update_descriptor_sets(&descriptor_writes, &[]);
        }
    }

    fn create_semaphore(&self) -> Option<vk::Semaphore> {
        unsafe {
            self.device
                .create_semaphore(&vk::SemaphoreCreateInfo::builder(), None)
                .ok()
        }
    }

    fn completion_status(&self, submission: VulkanGpuSubmission) -> GpuSubmissionCompletion {
        if !unsafe {
            self.device
                .get_fence_status(submission.fence)
                .unwrap_or(false)
        } {
            return GpuSubmissionCompletion::Pending;
        }

        let probe_plan = VulkanProbePlan::from_env();
        if probe_plan.map(|plan| plan.stage) == Some(VulkanProbeStage::AfterWaitBeforeReadback) {
            log::info!(
                "kairo-daemon: vulkan probe stop workload={:?} stage={} queue_mode={:?} memory_path={:?}",
                submission.workload,
                VulkanProbeStage::AfterWaitBeforeReadback.as_str(),
                submission.queue_mode,
                submission.memory_path
            );
            return GpuSubmissionCompletion::Completed(VulkanGpuCompletion {
                observations: VulkanObservationSet::default(),
                probe_stage: Some(VulkanProbeStage::AfterWaitBeforeReadback),
                cleanup_mode: probe_plan
                    .map(|plan| plan.cleanup_mode())
                    .unwrap_or(VulkanProbeCleanupMode::Cleanup),
            });
        }

        let observations = self.read_observations(submission);
        GpuSubmissionCompletion::Completed(VulkanGpuCompletion {
            observations,
            probe_stage: probe_plan.map(|plan| plan.stage),
            cleanup_mode: probe_plan
                .map(|plan| plan.cleanup_mode())
                .unwrap_or(VulkanProbeCleanupMode::Cleanup),
        })
    }

    fn cleanup_submission(
        &self,
        submission: VulkanGpuSubmission,
        zeroize_scope: Option<ZeroizeScope>,
    ) {
        if let Some(scope) = zeroize_scope {
            let _ = self.best_effort_zeroize_submission(submission, scope);
        }
        self.destroy_submission_resources(submission);
    }

    fn is_submission_complete(&self, submission: VulkanGpuSubmission) -> bool {
        unsafe {
            self.device
                .get_fence_status(submission.fence)
                .unwrap_or(false)
        }
    }

    fn best_effort_zeroize_submission(
        &self,
        submission: VulkanGpuSubmission,
        scope: ZeroizeScope,
    ) -> bool {
        let mut zeroized = false;
        match scope {
            ZeroizeScope::DeviceBuffers => {
                zeroized |= self.zeroize_buffer_if_host_visible(submission.resources.input);
                zeroized |= self.zeroize_buffer_if_host_visible(submission.resources.output);
            }
            ZeroizeScope::HostStagingBuffers => {
                zeroized |= submission
                    .resources
                    .input_staging
                    .map(|buffer| self.zeroize_buffer_if_host_visible(buffer))
                    .unwrap_or(false);
                zeroized |= submission
                    .resources
                    .output_staging
                    .map(|buffer| self.zeroize_buffer_if_host_visible(buffer))
                    .unwrap_or(false);
            }
            ZeroizeScope::AllTransientBuffers => {
                for buffer in [
                    Some(submission.resources.input),
                    Some(submission.resources.output),
                    submission.resources.input_staging,
                    submission.resources.output_staging,
                ]
                .into_iter()
                .flatten()
                {
                    zeroized |= self.zeroize_buffer_if_host_visible(buffer);
                }
            }
        }
        zeroized
    }

    fn destroy_submission_resources(&self, submission: VulkanGpuSubmission) {
        unsafe {
            self.device.destroy_fence(submission.fence, None);
            if let Some(semaphore) = submission.upload_complete_semaphore {
                self.device.destroy_semaphore(semaphore, None);
            }
            if let Some(semaphore) = submission.compute_complete_semaphore {
                self.device.destroy_semaphore(semaphore, None);
            }
            self.device.free_command_buffers(
                self.compute_command_pool,
                &[submission.compute_command_buffer],
            );
            if let Some(command_buffer) = submission.transfer_command_buffers[0] {
                self.device.free_command_buffers(
                    self.transfer_or_compute_command_pool(submission.queue_mode),
                    &[command_buffer],
                );
            }
            if let Some(command_buffer) = submission.transfer_command_buffers[1] {
                self.device.free_command_buffers(
                    self.transfer_or_compute_command_pool(submission.queue_mode),
                    &[command_buffer],
                );
            }
            self.device
                .destroy_descriptor_pool(submission.descriptor_pool, None);
            self.destroy_buffer_allocation(submission.resources.input);
            self.destroy_buffer_allocation(submission.resources.output);
            if let Some(buffer) = submission.resources.input_staging {
                self.destroy_buffer_allocation(buffer);
            }
            if let Some(buffer) = submission.resources.output_staging {
                self.destroy_buffer_allocation(buffer);
            }
        }
    }

    fn destroy_submission_resource_allocations(&self, resources: VulkanSubmissionResources) {
        self.destroy_buffer_allocation(resources.input);
        self.destroy_buffer_allocation(resources.output);
        if let Some(buffer) = resources.input_staging {
            self.destroy_buffer_allocation(buffer);
        }
        if let Some(buffer) = resources.output_staging {
            self.destroy_buffer_allocation(buffer);
        }
    }

    fn free_partial_command_buffers(
        &self,
        queue_mode: VulkanQueueRoutingMode,
        compute_command_buffer: vk::CommandBuffer,
        transfer_command_buffers: [Option<vk::CommandBuffer>; 2],
    ) {
        unsafe {
            self.device
                .free_command_buffers(self.compute_command_pool, &[compute_command_buffer]);
            if let Some(command_buffer) = transfer_command_buffers[0] {
                self.device.free_command_buffers(
                    self.transfer_or_compute_command_pool(queue_mode),
                    &[command_buffer],
                );
            }
            if let Some(command_buffer) = transfer_command_buffers[1] {
                self.device.free_command_buffers(
                    self.transfer_or_compute_command_pool(queue_mode),
                    &[command_buffer],
                );
            }
        }
    }

    fn create_host_visible_buffer(
        &self,
        size: u64,
        usage: vk::BufferUsageFlags,
        queue_mode: VulkanQueueRoutingMode,
    ) -> Option<VulkanBufferAllocation> {
        self.create_buffer(
            size,
            usage,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            queue_mode,
        )
    }

    fn create_device_local_buffer(
        &self,
        size: u64,
        usage: vk::BufferUsageFlags,
        queue_mode: VulkanQueueRoutingMode,
    ) -> Option<VulkanBufferAllocation> {
        self.create_buffer(
            size,
            usage,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            queue_mode,
        )
    }

    fn create_buffer(
        &self,
        size: u64,
        usage: vk::BufferUsageFlags,
        memory_properties: vk::MemoryPropertyFlags,
        queue_mode: VulkanQueueRoutingMode,
    ) -> Option<VulkanBufferAllocation> {
        let queue_family_indices = self.sharing_queue_family_indices(queue_mode);
        let mut buffer_info = vk::BufferCreateInfo::builder()
            .size(size.max(4))
            .usage(usage)
            .sharing_mode(if queue_family_indices.len() > 1 {
                vk::SharingMode::CONCURRENT
            } else {
                vk::SharingMode::EXCLUSIVE
            });
        if queue_family_indices.len() > 1 {
            buffer_info = buffer_info.queue_family_indices(&queue_family_indices);
        }
        let buffer = unsafe { self.device.create_buffer(&buffer_info, None).ok()? };
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let memory_type_index =
            self.find_memory_type_index(requirements.memory_type_bits, memory_properties)?;
        let allocate_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        let memory = match unsafe { self.device.allocate_memory(&allocate_info, None) } {
            Ok(memory) => memory,
            Err(_) => {
                unsafe {
                    self.device.destroy_buffer(buffer, None);
                }
                return None;
            }
        };
        if unsafe { self.device.bind_buffer_memory(buffer, memory, 0) }.is_err() {
            unsafe {
                self.device.destroy_buffer(buffer, None);
                self.device.free_memory(memory, None);
            }
            return None;
        }
        Some(VulkanBufferAllocation {
            buffer,
            memory,
            allocation_size: requirements.size,
            host_visible: memory_properties.contains(vk::MemoryPropertyFlags::HOST_VISIBLE),
        })
    }

    fn find_memory_type_index(
        &self,
        type_bits: u32,
        properties: vk::MemoryPropertyFlags,
    ) -> Option<u32> {
        let memory_properties = unsafe {
            self.instance
                .get_physical_device_memory_properties(self.physical_device)
        };
        (0..memory_properties.memory_type_count).find(|index| {
            let supported = (type_bits & (1 << index)) != 0;
            let flags = memory_properties.memory_types[*index as usize].property_flags;
            supported && flags.contains(properties)
        })
    }

    fn write_bytes_to_memory(&self, memory: vk::DeviceMemory, bytes: &[u8]) -> Option<()> {
        let mapped = unsafe {
            self.device
                .map_memory(memory, 0, bytes.len() as u64, vk::MemoryMapFlags::empty())
                .ok()?
        };
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped.cast::<u8>(), bytes.len());
            self.device.unmap_memory(memory);
        }
        Some(())
    }

    fn read_packet_observation(
        &self,
        submission: VulkanGpuSubmission,
    ) -> Option<VulkanPacketObservation> {
        let metrics = self.read_output_metrics(submission)?;
        Some(VulkanPacketObservation {
            total_bytes: metrics[0],
            ascii_bytes: metrics[1],
            delimiter_bytes: metrics[2],
            non_ascii_bytes: metrics[3],
            tech_threat_hits: metrics[4],
            harm_threat_hits: metrics[5],
            conceal_threat_hits: metrics[6],
        })
    }

    fn read_audit_observation(
        &self,
        submission: VulkanGpuSubmission,
    ) -> Option<VulkanAuditObservation> {
        let metrics = self.read_output_metrics(submission)?;
        Some(VulkanAuditObservation {
            total_words: metrics[0],
            non_zero_words: metrics[1],
            high_bit_words: metrics[2],
            xor_fold: metrics[3],
        })
    }

    fn read_output_metrics(&self, submission: VulkanGpuSubmission) -> Option<[u32; 7]> {
        let output_memory = submission
            .resources
            .output_staging
            .unwrap_or(submission.resources.output)
            .memory;
        let mapped = unsafe {
            self.device
                .map_memory(
                    output_memory,
                    0,
                    size_of::<[u32; 7]>() as u64,
                    vk::MemoryMapFlags::empty(),
                )
                .ok()?
        };
        let metrics = unsafe {
            let slice = slice::from_raw_parts(mapped.cast::<u32>(), 7);
            let values = [
                slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6],
            ];
            self.device.unmap_memory(output_memory);
            values
        };
        Some(metrics)
    }

    fn read_observations(&self, submission: VulkanGpuSubmission) -> VulkanObservationSet {
        match submission.workload {
            VulkanWorkloadClass::PacketPreclassification => VulkanObservationSet {
                packet_observation: self.read_packet_observation(submission),
                audit_observation: None,
            },
            VulkanWorkloadClass::MaintenanceHashing
            | VulkanWorkloadClass::AuditScan
            | VulkanWorkloadClass::BulkPrefilter => VulkanObservationSet {
                packet_observation: None,
                audit_observation: self.read_audit_observation(submission),
            },
        }
    }

    fn pipeline_for_workload(&self, workload: VulkanWorkloadClass) -> vk::Pipeline {
        match workload {
            VulkanWorkloadClass::PacketPreclassification => self.packet_compute_pipeline,
            VulkanWorkloadClass::MaintenanceHashing
            | VulkanWorkloadClass::AuditScan
            | VulkanWorkloadClass::BulkPrefilter => self.audit_compute_pipeline,
        }
    }

    fn queue_routing_mode(&self) -> VulkanQueueRoutingMode {
        match (
            self.tuning.split_transfer_enabled,
            self.transfer_queue,
            self.transfer_queue_family_index,
        ) {
            (true, Some(_), Some(index)) if index != self.compute_queue_family_index => {
                VulkanQueueRoutingMode::SplitTransferCompute
            }
            _ => VulkanQueueRoutingMode::ComputeOnly,
        }
    }

    fn transfer_or_compute_queue(&self, queue_mode: VulkanQueueRoutingMode) -> vk::Queue {
        if matches!(queue_mode, VulkanQueueRoutingMode::SplitTransferCompute) {
            self.transfer_queue.unwrap_or(self.compute_queue)
        } else {
            self.compute_queue
        }
    }

    fn transfer_or_compute_command_pool(
        &self,
        queue_mode: VulkanQueueRoutingMode,
    ) -> vk::CommandPool {
        if matches!(queue_mode, VulkanQueueRoutingMode::SplitTransferCompute) {
            self.transfer_command_pool
                .unwrap_or(self.compute_command_pool)
        } else {
            self.compute_command_pool
        }
    }

    fn sharing_queue_family_indices(&self, queue_mode: VulkanQueueRoutingMode) -> Vec<u32> {
        let mut indices = vec![self.compute_queue_family_index];
        if matches!(queue_mode, VulkanQueueRoutingMode::SplitTransferCompute) {
            if let Some(index) = self.transfer_queue_family_index {
                if index != self.compute_queue_family_index {
                    indices.push(index);
                }
            }
        }
        indices
    }

    fn destroy_buffer_allocation(&self, allocation: VulkanBufferAllocation) {
        unsafe {
            self.device.destroy_buffer(allocation.buffer, None);
            self.device.free_memory(allocation.memory, None);
        }
    }

    fn zeroize_buffer_if_host_visible(&self, allocation: VulkanBufferAllocation) -> bool {
        if !allocation.host_visible {
            return false;
        }
        let mapped = match unsafe {
            self.device.map_memory(
                allocation.memory,
                0,
                allocation.allocation_size,
                vk::MemoryMapFlags::empty(),
            )
        } {
            Ok(mapped) => mapped,
            Err(_) => return false,
        };
        unsafe {
            let bytes =
                slice::from_raw_parts_mut(mapped.cast::<u8>(), allocation.allocation_size as usize);
            zeroize_bytes(bytes);
            self.device.unmap_memory(allocation.memory);
        }
        true
    }
}

fn workload_counters_mut(
    counters: &mut VulkanDebugCounters,
    workload: VulkanWorkloadClass,
) -> &mut VulkanWorkloadCounters {
    match workload {
        VulkanWorkloadClass::PacketPreclassification => &mut counters.packet_preclassification,
        VulkanWorkloadClass::MaintenanceHashing => &mut counters.maintenance_hashing,
        VulkanWorkloadClass::AuditScan => &mut counters.audit_scan,
        VulkanWorkloadClass::BulkPrefilter => &mut counters.bulk_prefilter,
    }
}

fn record_submission_counters(
    counters: &mut VulkanDebugCounters,
    workload: VulkanWorkloadClass,
    path: VulkanExecutionPath,
    memory_path: Option<VulkanMemoryPath>,
    uploaded_bytes: u64,
    downloaded_bytes: u64,
) {
    counters.submissions = counters.submissions.saturating_add(1);
    counters.bytes_uploaded = counters.bytes_uploaded.saturating_add(uploaded_bytes);
    counters.bytes_downloaded = counters.bytes_downloaded.saturating_add(downloaded_bytes);
    let workload_counters = workload_counters_mut(counters, workload);
    workload_counters.submissions = workload_counters.submissions.saturating_add(1);
    workload_counters.bytes_uploaded = workload_counters
        .bytes_uploaded
        .saturating_add(uploaded_bytes);
    workload_counters.bytes_downloaded = workload_counters
        .bytes_downloaded
        .saturating_add(downloaded_bytes);
    match path {
        VulkanExecutionPath::Vulkan => {
            workload_counters.vulkan_selected = workload_counters.vulkan_selected.saturating_add(1);
            match memory_path {
                Some(VulkanMemoryPath::HostVisibleDirect) => {
                    workload_counters.host_visible_direct =
                        workload_counters.host_visible_direct.saturating_add(1);
                }
                Some(VulkanMemoryPath::DeviceLocalStaged) => {
                    workload_counters.device_local_staged =
                        workload_counters.device_local_staged.saturating_add(1);
                }
                None => {}
            }
        }
        VulkanExecutionPath::CpuFallback => {}
    }
}

fn record_completion_counters(
    counters: &mut VulkanDebugCounters,
    workload: VulkanWorkloadClass,
    path: VulkanExecutionPath,
    _memory_path: Option<VulkanMemoryPath>,
    latency: Duration,
) {
    match path {
        VulkanExecutionPath::Vulkan => {
            counters.vulkan_completions = counters.vulkan_completions.saturating_add(1);
        }
        VulkanExecutionPath::CpuFallback => {
            counters.cpu_fallbacks = counters.cpu_fallbacks.saturating_add(1);
        }
    }
    let workload_counters = workload_counters_mut(counters, workload);
    match path {
        VulkanExecutionPath::Vulkan => {
            workload_counters.vulkan_completed =
                workload_counters.vulkan_completed.saturating_add(1);
        }
        VulkanExecutionPath::CpuFallback => {
            workload_counters.cpu_fallbacks = workload_counters.cpu_fallbacks.saturating_add(1);
        }
    }
    let bucket = if latency < Duration::from_millis(1) {
        &mut workload_counters.latency_buckets.under_1ms
    } else if latency < Duration::from_millis(5) {
        &mut workload_counters.latency_buckets.under_5ms
    } else if latency < Duration::from_millis(20) {
        &mut workload_counters.latency_buckets.under_20ms
    } else {
        &mut workload_counters.latency_buckets.over_or_equal_20ms
    };
    *bucket = bucket.saturating_add(1);
}

impl Drop for VulkanPersistentRuntime {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device
                .destroy_pipeline(self.packet_compute_pipeline, None);
            self.device
                .destroy_pipeline(self.audit_compute_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device
                .destroy_command_pool(self.compute_command_pool, None);
            if let Some(pool) = self.transfer_command_pool {
                self.device.destroy_command_pool(pool, None);
            }
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

fn completion_status(stored: &VulkanStoredSubmission) -> VulkanPollStatus {
    if stored.fallback_reason == Some(VulkanFallbackReason::Timeout) {
        VulkanPollStatus::TimedOut
    } else {
        VulkanPollStatus::Completed
    }
}

fn simulated_gpu_duration(submission: &VulkanBatchSubmission) -> Duration {
    let extra = (submission.payload_len / (64 * 1024)) as u64;
    Duration::from_millis(DEFAULT_GPU_EXECUTION_MS + extra)
}

fn simulated_cpu_fallback_duration(submission: &VulkanBatchSubmission) -> Duration {
    let extra = (submission.payload_len / (256 * 1024)) as u64;
    Duration::from_millis(DEFAULT_CPU_FALLBACK_MS + extra)
}

fn suggested_pending_wait(now: Instant, ready_at: Instant, deadline: Instant) -> Duration {
    let until_ready = ready_at.saturating_duration_since(now);
    let until_deadline = deadline.saturating_duration_since(now);
    let bounded = until_ready
        .min(until_deadline)
        .min(Duration::from_millis(MAX_PENDING_POLL_MS));
    if bounded.is_zero() {
        Duration::from_millis(MIN_PENDING_POLL_MS)
    } else {
        bounded.max(Duration::from_millis(MIN_PENDING_POLL_MS))
    }
}

fn dispatch_group_count(submission: &VulkanBatchSubmission) -> u32 {
    let base = match submission.workload {
        VulkanWorkloadClass::PacketPreclassification => submission
            .surface_words
            .as_ref()
            .map(|words| words.len().max(64))
            .unwrap_or(submission.payload_len.max(4096)),
        VulkanWorkloadClass::MaintenanceHashing => submission.payload_len.max(64 * 1024),
        VulkanWorkloadClass::AuditScan => submission.payload_len.max(16 * 1024),
        VulkanWorkloadClass::BulkPrefilter => submission.payload_len.max(32 * 1024),
    };
    ((base + 63) / 64).clamp(1, 4096) as u32
}

fn workload_shader_label(workload: VulkanWorkloadClass) -> &'static str {
    match workload {
        VulkanWorkloadClass::PacketPreclassification => "packet_prefilter",
        VulkanWorkloadClass::MaintenanceHashing
        | VulkanWorkloadClass::AuditScan
        | VulkanWorkloadClass::BulkPrefilter => "audit_prefilter",
    }
}

fn initialize_vulkan_runtime() -> (VulkanBackendCapabilities, Option<VulkanPersistentRuntime>) {
    let probed = detect_vulkan_capabilities();
    let loader_hint = probed
        .driver_name
        .strip_prefix("loader:")
        .and_then(|tail| tail.split_whitespace().next())
        .map(str::to_string);
    let Some(runtime) = create_persistent_runtime(loader_hint.as_deref()) else {
        return (probed, None);
    };
    let capabilities = VulkanBackendCapabilities {
        compute_available: true,
        transfer_available: true,
        dedicated_zeroize_supported: probed.dedicated_zeroize_supported,
        driver_name: probed.driver_name,
        device_name: probed.device_name,
        compute_queue_family_index: Some(runtime.compute_queue_family_index),
        transfer_queue_family_index: Some(
            runtime
                .transfer_queue_family_index
                .unwrap_or(runtime.compute_queue_family_index),
        ),
    };
    (capabilities, Some(runtime))
}

fn create_persistent_runtime(loader_hint: Option<&str>) -> Option<VulkanPersistentRuntime> {
    let tuning = VulkanRuntimeTuning {
        split_transfer_enabled: !env_flag("KAIRO_VULKAN_FORCE_COMPUTE_ONLY").unwrap_or(false),
        device_local_fast_path: !env_flag("KAIRO_VULKAN_FORCE_HOST_VISIBLE").unwrap_or(false)
            && !env_flag("KAIRO_VULKAN_DISABLE_DEVICE_LOCAL").unwrap_or(false),
    };
    let entry = match unsafe { Entry::load() } {
        Ok(entry) => entry,
        Err(err) => {
            log::debug!(
                "kairo-daemon: failed to load vulkan entry for runtime loader_hint={:?}: {}",
                loader_hint,
                err
            );
            return None;
        }
    };

    let app_name = CString::new("kairo-daemon").ok()?;
    let engine_name = CString::new("tuff-kairo").ok()?;
    let app_info = vk::ApplicationInfo::builder()
        .application_name(&app_name)
        .application_version(vk::make_api_version(0, 0, 1, 0))
        .engine_name(&engine_name)
        .engine_version(vk::make_api_version(0, 0, 1, 0))
        .api_version(vk::make_api_version(0, 1, 0, 0));
    let create_info = vk::InstanceCreateInfo::builder().application_info(&app_info);
    let instance = unsafe { entry.create_instance(&create_info, None).ok()? };

    let physical_device = unsafe {
        instance
            .enumerate_physical_devices()
            .ok()?
            .into_iter()
            .next()?
    };
    let queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    let compute_queue_family_index = queue_families
        .iter()
        .enumerate()
        .find(|(_, family)| family.queue_flags.contains(vk::QueueFlags::COMPUTE))
        .map(|(index, _)| index as u32)?;
    let transfer_queue_family_index = queue_families
        .iter()
        .enumerate()
        .find(|(_, family)| family.queue_flags.contains(vk::QueueFlags::TRANSFER))
        .map(|(index, _)| index as u32);

    let mut unique_indices = vec![compute_queue_family_index];
    if let Some(index) = transfer_queue_family_index {
        if !unique_indices.contains(&index) {
            unique_indices.push(index);
        }
    }
    let queue_priority = [1.0f32];
    let queue_infos: Vec<_> = unique_indices
        .iter()
        .map(|index| {
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(*index)
                .queue_priorities(&queue_priority)
                .build()
        })
        .collect();
    let device_info = vk::DeviceCreateInfo::builder().queue_create_infos(&queue_infos);
    let device = match unsafe { instance.create_device(physical_device, &device_info, None) } {
        Ok(device) => device,
        Err(err) => {
            log::debug!(
                "kairo-daemon: failed to create persistent logical device loader_hint={:?}: {:?}",
                loader_hint,
                err
            );
            unsafe { instance.destroy_instance(None) };
            return None;
        }
    };

    let compute_queue = unsafe { device.get_device_queue(compute_queue_family_index, 0) };
    let transfer_queue =
        transfer_queue_family_index.map(|index| unsafe { device.get_device_queue(index, 0) });
    let compute_command_pool_info = vk::CommandPoolCreateInfo::builder()
        .queue_family_index(compute_queue_family_index)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
    let compute_command_pool =
        match unsafe { device.create_command_pool(&compute_command_pool_info, None) } {
            Ok(pool) => pool,
            Err(_) => {
                unsafe {
                    device.destroy_device(None);
                    instance.destroy_instance(None);
                }
                return None;
            }
        };
    let transfer_command_pool = match (tuning.split_transfer_enabled, transfer_queue_family_index) {
        (true, Some(index)) if index != compute_queue_family_index => {
            let transfer_command_pool_info = vk::CommandPoolCreateInfo::builder()
                .queue_family_index(index)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
            match unsafe { device.create_command_pool(&transfer_command_pool_info, None) } {
                Ok(pool) => Some(pool),
                Err(_) => {
                    unsafe {
                        device.destroy_command_pool(compute_command_pool, None);
                        device.destroy_device(None);
                        instance.destroy_instance(None);
                    }
                    return None;
                }
            }
        }
        _ => None,
    };

    let packet_shader_words =
        ash::util::read_spv(&mut std::io::Cursor::new(PACKET_PREFILTER_SHADER_SPV)).ok()?;
    let packet_shader_module_info =
        vk::ShaderModuleCreateInfo::builder().code(&packet_shader_words);
    let packet_shader_module =
        match unsafe { device.create_shader_module(&packet_shader_module_info, None) } {
            Ok(module) => module,
            Err(_) => {
                unsafe {
                    if let Some(pool) = transfer_command_pool {
                        device.destroy_command_pool(pool, None);
                    }
                    device.destroy_command_pool(compute_command_pool, None);
                    device.destroy_device(None);
                    instance.destroy_instance(None);
                }
                return None;
            }
        };
    let audit_shader_words =
        ash::util::read_spv(&mut std::io::Cursor::new(AUDIT_PREFILTER_SHADER_SPV)).ok()?;
    let audit_shader_module_info = vk::ShaderModuleCreateInfo::builder().code(&audit_shader_words);
    let audit_shader_module =
        match unsafe { device.create_shader_module(&audit_shader_module_info, None) } {
            Ok(module) => module,
            Err(_) => {
                unsafe {
                    device.destroy_shader_module(packet_shader_module, None);
                    if let Some(pool) = transfer_command_pool {
                        device.destroy_command_pool(pool, None);
                    }
                    device.destroy_command_pool(compute_command_pool, None);
                    device.destroy_device(None);
                    instance.destroy_instance(None);
                }
                return None;
            }
        };
    let descriptor_bindings = [
        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .build(),
        vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .build(),
    ];
    let descriptor_set_layout_info =
        vk::DescriptorSetLayoutCreateInfo::builder().bindings(&descriptor_bindings);
    let descriptor_set_layout =
        match unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_info, None) } {
            Ok(layout) => layout,
            Err(_) => {
                unsafe {
                    device.destroy_shader_module(packet_shader_module, None);
                    device.destroy_shader_module(audit_shader_module, None);
                    if let Some(pool) = transfer_command_pool {
                        device.destroy_command_pool(pool, None);
                    }
                    device.destroy_command_pool(compute_command_pool, None);
                    device.destroy_device(None);
                    instance.destroy_instance(None);
                }
                return None;
            }
        };
    let push_constant_ranges = [vk::PushConstantRange::builder()
        .stage_flags(vk::ShaderStageFlags::COMPUTE)
        .offset(0)
        .size(size_of::<PrefilterPushConstants>() as u32)
        .build()];
    let pipeline_layout_set_layouts = [descriptor_set_layout];
    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::builder()
        .set_layouts(&pipeline_layout_set_layouts)
        .push_constant_ranges(&push_constant_ranges);
    let pipeline_layout =
        match unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) } {
            Ok(layout) => layout,
            Err(_) => {
                unsafe {
                    device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                    device.destroy_shader_module(packet_shader_module, None);
                    device.destroy_shader_module(audit_shader_module, None);
                    if let Some(pool) = transfer_command_pool {
                        device.destroy_command_pool(pool, None);
                    }
                    device.destroy_command_pool(compute_command_pool, None);
                    device.destroy_device(None);
                    instance.destroy_instance(None);
                }
                return None;
            }
        };
    let main = CString::new("main").ok()?;
    let packet_stage = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(packet_shader_module)
        .name(&main);
    let packet_pipeline_info = vk::ComputePipelineCreateInfo::builder()
        .stage(packet_stage.build())
        .layout(pipeline_layout);
    let packet_compute_pipeline = match unsafe {
        device.create_compute_pipelines(
            vk::PipelineCache::null(),
            &[packet_pipeline_info.build()],
            None,
        )
    } {
        Ok(mut pipelines) => pipelines.remove(0),
        Err(_) => {
            unsafe {
                device.destroy_pipeline_layout(pipeline_layout, None);
                device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                device.destroy_shader_module(packet_shader_module, None);
                device.destroy_shader_module(audit_shader_module, None);
                if let Some(pool) = transfer_command_pool {
                    device.destroy_command_pool(pool, None);
                }
                device.destroy_command_pool(compute_command_pool, None);
                device.destroy_device(None);
                instance.destroy_instance(None);
            }
            return None;
        }
    };
    let audit_stage = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(audit_shader_module)
        .name(&main);
    let audit_pipeline_info = vk::ComputePipelineCreateInfo::builder()
        .stage(audit_stage.build())
        .layout(pipeline_layout);
    let audit_compute_pipeline = match unsafe {
        device.create_compute_pipelines(
            vk::PipelineCache::null(),
            &[audit_pipeline_info.build()],
            None,
        )
    } {
        Ok(mut pipelines) => pipelines.remove(0),
        Err(_) => {
            unsafe {
                device.destroy_pipeline(packet_compute_pipeline, None);
                device.destroy_pipeline_layout(pipeline_layout, None);
                device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                device.destroy_shader_module(packet_shader_module, None);
                device.destroy_shader_module(audit_shader_module, None);
                if let Some(pool) = transfer_command_pool {
                    device.destroy_command_pool(pool, None);
                }
                device.destroy_command_pool(compute_command_pool, None);
                device.destroy_device(None);
                instance.destroy_instance(None);
            }
            return None;
        }
    };
    unsafe {
        device.destroy_shader_module(packet_shader_module, None);
        device.destroy_shader_module(audit_shader_module, None);
    }

    Some(VulkanPersistentRuntime {
        _entry: entry,
        instance,
        device,
        physical_device,
        compute_queue,
        transfer_queue,
        compute_command_pool,
        transfer_command_pool,
        descriptor_set_layout,
        pipeline_layout,
        packet_compute_pipeline,
        audit_compute_pipeline,
        compute_queue_family_index,
        transfer_queue_family_index,
        tuning,
    })
}

fn detect_vulkan_capabilities() -> VulkanBackendCapabilities {
    let loader_override = env_nonempty("KAIRO_VULKAN_LOADER_PATH");
    let device_override = env_nonempty("KAIRO_VULKAN_DEVICE_NAME");
    let driver_override = env_nonempty("KAIRO_VULKAN_DRIVER_NAME");
    let render_override = env_nonempty("KAIRO_VULKAN_RENDER_NODE");
    let force_compute = env_flag("KAIRO_VULKAN_FORCE_COMPUTE").unwrap_or(false);
    let force_zeroize = env_flag("KAIRO_VULKAN_FORCE_ZEROIZE").unwrap_or(false);

    let loader_path = loader_override
        .filter(|path| Path::new(path).exists())
        .or_else(|| find_first_existing(&VULKAN_LOADER_CANDIDATES));
    let render_node = render_override
        .filter(|path| Path::new(path).exists())
        .or_else(|| find_first_existing(&RENDER_NODE_CANDIDATES));

    let runtime = loader_path
        .as_deref()
        .and_then(|hint| probe_vulkan_runtime(Some(hint)))
        .or_else(|| {
            loader_path
                .is_none()
                .then(|| probe_vulkan_runtime(None))
                .flatten()
        });

    let transfer_available = runtime
        .as_ref()
        .map(|runtime| runtime.transfer_queue_family_index.is_some())
        .unwrap_or(loader_path.is_some());
    let compute_available = force_compute
        || runtime
            .as_ref()
            .map(|runtime| runtime.compute_queue_family_index.is_some())
            .unwrap_or(transfer_available && render_node.is_some());
    let driver_name = driver_override.unwrap_or_else(|| {
        runtime
            .as_ref()
            .map(|runtime| runtime.driver_name.clone())
            .or_else(|| loader_path.clone().map(|path| format!("loader:{}", path)))
            .unwrap_or_else(|| "cpu-fallback-contract".to_string())
    });
    let device_name = device_override.unwrap_or_else(|| {
        runtime
            .as_ref()
            .map(|runtime| runtime.device_name.clone())
            .or_else(|| {
                render_node
                    .clone()
                    .map(|path| format!("render-node:{}", path))
            })
            .unwrap_or_else(|| "unbound".to_string())
    });

    VulkanBackendCapabilities {
        compute_available,
        transfer_available,
        dedicated_zeroize_supported: force_zeroize && compute_available,
        driver_name,
        device_name,
        compute_queue_family_index: runtime
            .as_ref()
            .and_then(|runtime| runtime.compute_queue_family_index),
        transfer_queue_family_index: runtime
            .as_ref()
            .and_then(|runtime| runtime.transfer_queue_family_index),
    }
}

#[derive(Debug, Clone)]
struct VulkanRuntimeProbe {
    driver_name: String,
    device_name: String,
    compute_queue_family_index: Option<u32>,
    transfer_queue_family_index: Option<u32>,
}

fn probe_vulkan_runtime(loader_hint: Option<&str>) -> Option<VulkanRuntimeProbe> {
    let entry = match unsafe { Entry::load() } {
        Ok(entry) => entry,
        Err(err) => {
            log::debug!(
                "kairo-daemon: failed to load vulkan entry loader_hint={:?}: {}",
                loader_hint,
                err
            );
            return None;
        }
    };

    let app_name = CString::new("kairo-daemon").ok()?;
    let engine_name = CString::new("tuff-kairo").ok()?;
    let app_info = vk::ApplicationInfo::builder()
        .application_name(&app_name)
        .application_version(vk::make_api_version(0, 0, 1, 0))
        .engine_name(&engine_name)
        .engine_version(vk::make_api_version(0, 0, 1, 0))
        .api_version(vk::make_api_version(0, 1, 0, 0));
    let create_info = vk::InstanceCreateInfo::builder().application_info(&app_info);

    let instance = match unsafe { entry.create_instance(&create_info, None) } {
        Ok(instance) => instance,
        Err(err) => {
            log::debug!(
                "kairo-daemon: failed to create vulkan instance loader_hint={:?}: {:?}",
                loader_hint,
                err
            );
            return None;
        }
    };

    let result = unsafe {
        let physical_devices = match instance.enumerate_physical_devices() {
            Ok(devices) => devices,
            Err(err) => {
                log::debug!(
                    "kairo-daemon: failed to enumerate physical devices loader_hint={:?}: {:?}",
                    loader_hint,
                    err
                );
                instance.destroy_instance(None);
                return None;
            }
        };
        let physical_device = match physical_devices.first().copied() {
            Some(device) => device,
            None => {
                instance.destroy_instance(None);
                return None;
            }
        };
        let properties = instance.get_physical_device_properties(physical_device);
        let queue_families = instance.get_physical_device_queue_family_properties(physical_device);
        let compute_queue_family_index = queue_families
            .iter()
            .enumerate()
            .find(|(_, family)| family.queue_flags.contains(vk::QueueFlags::COMPUTE))
            .map(|(index, _)| index as u32);
        let transfer_queue_family_index = queue_families
            .iter()
            .enumerate()
            .find(|(_, family)| family.queue_flags.contains(vk::QueueFlags::TRANSFER))
            .map(|(index, _)| index as u32);
        let device_name = CStr::from_ptr(properties.device_name.as_ptr())
            .to_string_lossy()
            .into_owned();
        let api_major = vk::api_version_major(properties.api_version);
        let api_minor = vk::api_version_minor(properties.api_version);
        let api_patch = vk::api_version_patch(properties.api_version);
        let driver_name = loader_hint
            .map(|hint| {
                format!(
                    "loader:{} vk{}.{}.{}",
                    hint, api_major, api_minor, api_patch
                )
            })
            .unwrap_or_else(|| {
                format!("vulkan-loader vk{}.{}.{}", api_major, api_minor, api_patch)
            });
        if !probe_logical_device_and_queues(
            &instance,
            physical_device,
            compute_queue_family_index,
            transfer_queue_family_index,
        ) {
            instance.destroy_instance(None);
            return None;
        }
        instance.destroy_instance(None);
        Some(VulkanRuntimeProbe {
            driver_name,
            device_name,
            compute_queue_family_index,
            transfer_queue_family_index,
        })
    };

    result
}

fn probe_logical_device_and_queues(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    compute_queue_family_index: Option<u32>,
    transfer_queue_family_index: Option<u32>,
) -> bool {
    let mut unique_indices = Vec::new();
    if let Some(index) = compute_queue_family_index {
        unique_indices.push(index);
    }
    if let Some(index) = transfer_queue_family_index {
        if !unique_indices.contains(&index) {
            unique_indices.push(index);
        }
    }
    if unique_indices.is_empty() {
        return false;
    }

    let queue_priority = [1.0f32];
    let queue_infos: Vec<_> = unique_indices
        .iter()
        .map(|index| {
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(*index)
                .queue_priorities(&queue_priority)
                .build()
        })
        .collect();
    let device_info = vk::DeviceCreateInfo::builder().queue_create_infos(&queue_infos);

    let device = match unsafe { instance.create_device(physical_device, &device_info, None) } {
        Ok(device) => device,
        Err(err) => {
            log::debug!(
                "kairo-daemon: failed to create logical device for vulkan probe: {:?}",
                err
            );
            return false;
        }
    };

    if let Some(index) = compute_queue_family_index {
        let _ = unsafe { device.get_device_queue(index, 0) };
    }
    if let Some(index) = transfer_queue_family_index {
        let _ = unsafe { device.get_device_queue(index, 0) };
    }
    unsafe {
        device.destroy_device(None);
    }
    true
}

fn find_first_existing(candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|candidate| Path::new(candidate).exists())
        .map(|candidate| (*candidate).to_string())
}

fn env_nonempty(name: &str) -> Option<String> {
    env::var(name).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn env_usize(name: &str) -> Option<usize> {
    env_nonempty(name).and_then(|raw| raw.parse::<usize>().ok())
}

fn env_flag(name: &str) -> Option<bool> {
    env_nonempty(name).map(|raw| matches!(raw.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn pack_surface_words(bytes: &[u8]) -> Vec<u32> {
    let mut words = Vec::with_capacity(bytes.len().div_ceil(4).max(1));
    for chunk in bytes.chunks(4) {
        let mut word_bytes = [0u8; 4];
        word_bytes[..chunk.len()].copy_from_slice(chunk);
        words.push(u32::from_le_bytes(word_bytes));
    }
    if words.is_empty() {
        words.push(0);
    }
    words
}

fn size_of_val_bytes<T>(values: &[T]) -> u64 {
    (values.len() * size_of::<T>()) as u64
}

fn u32_slice_as_bytes(values: &[u32]) -> &[u8] {
    unsafe {
        slice::from_raw_parts(
            values.as_ptr().cast::<u8>(),
            size_of_val_bytes(values) as usize,
        )
    }
}

fn any_as_bytes<T>(value: &T) -> &[u8] {
    unsafe { slice::from_raw_parts((value as *const T).cast::<u8>(), size_of::<T>()) }
}

fn zeroize_bytes(bytes: &mut [u8]) {
    bytes.fill(0);
    black_box(());
}

fn observation_slots_for_workload(workload: VulkanWorkloadClass) -> (bool, bool) {
    match workload {
        VulkanWorkloadClass::PacketPreclassification => (true, false),
        VulkanWorkloadClass::MaintenanceHashing
        | VulkanWorkloadClass::AuditScan
        | VulkanWorkloadClass::BulkPrefilter => (false, true),
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VulkanCleanupManifest {
    command_buffers: usize,
    semaphores: usize,
    buffers: usize,
    memories: usize,
    descriptor_pools: usize,
    fences: usize,
}

#[cfg(test)]
fn cleanup_manifest(submission: VulkanGpuSubmission) -> VulkanCleanupManifest {
    VulkanCleanupManifest {
        command_buffers: 1 + submission.transfer_command_buffers.iter().flatten().count(),
        semaphores: submission.upload_complete_semaphore.iter().count()
            + submission.compute_complete_semaphore.iter().count(),
        buffers: 2
            + submission.resources.input_staging.iter().count()
            + submission.resources.output_staging.iter().count(),
        memories: 2
            + submission.resources.input_staging.iter().count()
            + submission.resources.output_staging.iter().count(),
        descriptor_pools: usize::from(submission.descriptor_pool != vk::DescriptorPool::null()),
        fences: usize::from(submission.fence != vk::Fence::null()),
    }
}

pub fn global_backend() -> &'static Arc<VulkanBackend> {
    &GLOBAL_BACKEND
}

pub fn init_vulkan_offload() {
    let _ = global_backend().initialize();
}

pub struct VulkanFuture {
    inner: Pin<Box<dyn Future<Output = bool> + Send>>,
}

impl VulkanFuture {
    pub fn new() -> Self {
        let backend = Arc::clone(global_backend());
        if matches!(backend.state(), VulkanBackendState::Uninitialized) {
            let _ = backend.initialize();
        }
        let handle = backend.submit_batch(VulkanBatchSubmission::maintenance_prefilter(
            DEFAULT_MIN_BATCH_BYTES,
        ));
        let future = async move {
            let result = backend.wait_for_completion(handle).await;
            matches!(result.path, VulkanExecutionPath::Vulkan)
        };
        Self {
            inner: Box::pin(future),
        }
    }
}

impl Future for VulkanFuture {
    type Output = bool;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ash::vk::Handle;
    use std::sync::Mutex;

    static TEST_GUARD: Mutex<()> = Mutex::new(());

    fn fallback_test_backend() -> VulkanBackend {
        VulkanBackend::new(VulkanBackendConfig {
            enable_vulkan: false,
            packet_preclassification_min_batch_bytes: DEFAULT_PACKET_MIN_BATCH_BYTES,
            maintenance_hashing_min_batch_bytes: DEFAULT_MAINTENANCE_MIN_BATCH_BYTES,
            audit_scan_min_batch_bytes: DEFAULT_AUDIT_MIN_BATCH_BYTES,
            bulk_prefilter_min_batch_bytes: DEFAULT_BULK_MIN_BATCH_BYTES,
            submit_timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
        })
    }

    fn probe_ready_test_backend() -> VulkanBackend {
        let backend = VulkanBackend::new(VulkanBackendConfig {
            enable_vulkan: true,
            packet_preclassification_min_batch_bytes: DEFAULT_PACKET_MIN_BATCH_BYTES,
            maintenance_hashing_min_batch_bytes: DEFAULT_MAINTENANCE_MIN_BATCH_BYTES,
            audit_scan_min_batch_bytes: DEFAULT_AUDIT_MIN_BATCH_BYTES,
            bulk_prefilter_min_batch_bytes: DEFAULT_BULK_MIN_BATCH_BYTES,
            submit_timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
        });
        {
            let mut inner = backend.inner.lock();
            inner.state = VulkanBackendState::Ready;
            inner.capabilities.compute_available = true;
            inner.capabilities.transfer_available = true;
            inner.capabilities.driver_name = "probe-ready".to_string();
            inner.capabilities.device_name = "probe-ready".to_string();
            inner.capabilities.compute_queue_family_index = Some(0);
            inner.capabilities.transfer_queue_family_index = Some(0);
        }
        backend
    }

    fn with_probe_env<T>(stage: Option<&str>, skip_cleanup: bool, f: impl FnOnce() -> T) -> T {
        let old_stage = env::var("KAIRO_VULKAN_PROBE_STAGE").ok();
        let old_skip_cleanup = env::var("KAIRO_VULKAN_PROBE_SKIP_CLEANUP").ok();
        match stage {
            Some(value) => env::set_var("KAIRO_VULKAN_PROBE_STAGE", value),
            None => env::remove_var("KAIRO_VULKAN_PROBE_STAGE"),
        }
        if skip_cleanup {
            env::set_var("KAIRO_VULKAN_PROBE_SKIP_CLEANUP", "1");
        } else {
            env::remove_var("KAIRO_VULKAN_PROBE_SKIP_CLEANUP");
        }
        let result = f();
        match old_stage {
            Some(value) => env::set_var("KAIRO_VULKAN_PROBE_STAGE", value),
            None => env::remove_var("KAIRO_VULKAN_PROBE_STAGE"),
        }
        match old_skip_cleanup {
            Some(value) => env::set_var("KAIRO_VULKAN_PROBE_SKIP_CLEANUP", value),
            None => env::remove_var("KAIRO_VULKAN_PROBE_SKIP_CLEANUP"),
        }
        result
    }

    fn probe_capabilities_for_test() -> VulkanBackendCapabilities {
        let loader = env::var("KAIRO_VULKAN_LOADER_PATH").ok();
        let render = env::var("KAIRO_VULKAN_RENDER_NODE").ok();
        let force = env::var("KAIRO_VULKAN_FORCE_COMPUTE").ok();
        let zeroize = env::var("KAIRO_VULKAN_FORCE_ZEROIZE").ok();
        env::set_var("KAIRO_VULKAN_LOADER_PATH", "/bin/sh");
        env::set_var("KAIRO_VULKAN_RENDER_NODE", "/bin/sh");
        env::set_var("KAIRO_VULKAN_FORCE_COMPUTE", "1");
        env::set_var("KAIRO_VULKAN_FORCE_ZEROIZE", "1");
        let detected = detect_vulkan_capabilities();
        match loader {
            Some(value) => env::set_var("KAIRO_VULKAN_LOADER_PATH", value),
            None => env::remove_var("KAIRO_VULKAN_LOADER_PATH"),
        }
        match render {
            Some(value) => env::set_var("KAIRO_VULKAN_RENDER_NODE", value),
            None => env::remove_var("KAIRO_VULKAN_RENDER_NODE"),
        }
        match force {
            Some(value) => env::set_var("KAIRO_VULKAN_FORCE_COMPUTE", value),
            None => env::remove_var("KAIRO_VULKAN_FORCE_COMPUTE"),
        }
        match zeroize {
            Some(value) => env::set_var("KAIRO_VULKAN_FORCE_ZEROIZE", value),
            None => env::remove_var("KAIRO_VULKAN_FORCE_ZEROIZE"),
        }
        detected
    }

    #[tokio::test]
    async fn disallows_single_block_sync_path() {
        let _guard = TEST_GUARD.lock().unwrap();
        let backend = fallback_test_backend();
        backend.initialize();
        let handle = backend.submit_batch(VulkanBatchSubmission {
            workload: VulkanWorkloadClass::MaintenanceHashing,
            queue: VulkanQueueClass::ComputeOnly,
            payload_len: 4096,
            surface_words: None,
            timeout: Duration::from_millis(10),
            requires_zeroize: false,
            allows_gpu: true,
            is_boot_or_recovery_path: false,
            is_truth_boundary: false,
            is_single_block_sync: true,
        });
        let polled = backend.poll_completion(handle);
        assert!(matches!(
            polled.status,
            VulkanPollStatus::Pending | VulkanPollStatus::Completed
        ));
        let result = backend.wait_for_completion(handle).await;
        assert_eq!(result.path, VulkanExecutionPath::CpuFallback);
        assert_eq!(
            result.fallback_reason,
            Some(VulkanFallbackReason::DisabledByPolicy)
        );
    }

    #[tokio::test]
    async fn reports_pending_before_async_completion() {
        let _guard = TEST_GUARD.lock().unwrap();
        let backend = fallback_test_backend();
        backend.initialize();
        let handle = backend.submit_batch(VulkanBatchSubmission {
            workload: VulkanWorkloadClass::AuditScan,
            queue: VulkanQueueClass::Any,
            payload_len: DEFAULT_MIN_BATCH_BYTES,
            surface_words: None,
            timeout: Duration::from_millis(100),
            requires_zeroize: true,
            allows_gpu: true,
            is_boot_or_recovery_path: false,
            is_truth_boundary: false,
            is_single_block_sync: false,
        });
        let first = backend.poll_completion(handle);
        assert_eq!(first.status, VulkanPollStatus::Pending);
        let result = backend.wait_for_completion(handle).await;
        assert!(matches!(
            result.path,
            VulkanExecutionPath::Vulkan | VulkanExecutionPath::CpuFallback
        ));
    }

    #[tokio::test]
    async fn zeroize_hook_clears_pending_requirement() {
        let _guard = TEST_GUARD.lock().unwrap();
        let backend = fallback_test_backend();
        backend.initialize();
        let handle = backend.submit_batch(VulkanBatchSubmission {
            workload: VulkanWorkloadClass::AuditScan,
            queue: VulkanQueueClass::Any,
            payload_len: DEFAULT_MIN_BATCH_BYTES,
            surface_words: None,
            timeout: Duration::from_millis(10),
            requires_zeroize: true,
            allows_gpu: true,
            is_boot_or_recovery_path: false,
            is_truth_boundary: false,
            is_single_block_sync: false,
        });
        assert!(backend.request_zeroize(VulkanZeroizeRequest {
            handle,
            scope: ZeroizeScope::AllTransientBuffers,
        }));
        let result = backend.wait_for_completion(handle).await;
        assert!(!result.zeroize_required);
    }

    #[tokio::test]
    async fn debug_counters_record_cpu_fallbacks_and_zeroize_requests() {
        let _guard = TEST_GUARD.lock().unwrap();
        let backend = fallback_test_backend();
        backend.initialize();
        let handle = backend.submit_batch(VulkanBatchSubmission {
            workload: VulkanWorkloadClass::PacketPreclassification,
            queue: VulkanQueueClass::ComputeOnly,
            payload_len: 64,
            surface_words: Some(vec![1, 2, 3, 4]),
            timeout: Duration::from_millis(10),
            requires_zeroize: true,
            allows_gpu: true,
            is_boot_or_recovery_path: false,
            is_truth_boundary: false,
            is_single_block_sync: false,
        });
        assert!(backend.request_zeroize(VulkanZeroizeRequest {
            handle,
            scope: ZeroizeScope::HostStagingBuffers,
        }));
        let _ = backend.wait_for_completion(handle).await;
        let counters = backend.debug_counters();
        assert_eq!(counters.submissions, 1);
        assert_eq!(counters.zeroize_requests, 1);
        assert_eq!(counters.cpu_fallbacks, 1);
        assert_eq!(counters.packet_preclassification.submissions, 1);
        assert_eq!(counters.packet_preclassification.cpu_fallbacks, 1);
    }

    #[tokio::test]
    async fn completed_submission_is_released_after_wait() {
        let _guard = TEST_GUARD.lock().unwrap();
        let backend = fallback_test_backend();
        backend.initialize();
        let handle = backend.submit_batch(VulkanBatchSubmission {
            workload: VulkanWorkloadClass::AuditScan,
            queue: VulkanQueueClass::Any,
            payload_len: DEFAULT_MIN_BATCH_BYTES,
            surface_words: None,
            timeout: Duration::from_millis(10),
            requires_zeroize: false,
            allows_gpu: true,
            is_boot_or_recovery_path: false,
            is_truth_boundary: false,
            is_single_block_sync: false,
        });
        let _ = backend.wait_for_completion(handle).await;
        assert_eq!(
            backend.poll_completion(handle).status,
            VulkanPollStatus::Missing
        );
    }

    #[test]
    fn probe_plan_is_noop_when_unset() {
        let _guard = TEST_GUARD.lock().unwrap();
        with_probe_env(None, false, || {
            assert_eq!(VulkanProbePlan::from_env(), None);
        });
    }

    #[test]
    fn probe_plan_parses_stage_and_skip_cleanup() {
        let _guard = TEST_GUARD.lock().unwrap();
        with_probe_env(Some("after_wait_before_readback"), true, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::AfterWaitBeforeReadback);
            assert!(plan.skip_cleanup);
            assert_eq!(plan.cleanup_mode(), VulkanProbeCleanupMode::SkipCleanup);
        });
    }

    #[test]
    fn probe_plan_parses_after_command_record_before_cleanup() {
        let _guard = TEST_GUARD.lock().unwrap();
        with_probe_env(Some("after_command_record_before_cleanup"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(
                plan.stage,
                VulkanProbeStage::AfterCommandRecordBeforeCleanup
            );
            assert_eq!(plan.cleanup_mode(), VulkanProbeCleanupMode::Cleanup);
        });
    }

    #[test]
    fn probe_plan_parses_begin_command_buffer_stages() {
        let _guard = TEST_GUARD.lock().unwrap();
        with_probe_env(Some("after_fence_create"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::AfterFenceCreate);
        });
        with_probe_env(Some("after_fence_create_before_cleanup"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::AfterFenceCreateBeforeCleanup);
        });
        with_probe_env(Some("before_create_fence"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::BeforeCreateFence);
        });
        with_probe_env(Some("before_prefence_match_enter"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::BeforePreFenceMatchEnter);
        });
        with_probe_env(
            Some("after_descriptor_update_before_probe_stage_read"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::AfterDescriptorUpdateBeforeProbeStageRead
                );
            },
        );
        with_probe_env(
            Some("after_probe_stage_read_before_prefence_branch_eval"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::AfterProbeStageReadBeforePreFenceBranchEval
                );
            },
        );
        with_probe_env(
            Some("before_prefence_match_enter_with_cleanup"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::BeforePreFenceMatchEnterWithCleanup
                );
            },
        );
        with_probe_env(
            Some("before_prefence_match_enter_after_cleanup_before_return"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::BeforePreFenceMatchEnterAfterCleanupBeforeReturn
                );
            },
        );
        with_probe_env(
            Some("after_prefence_match_enter_before_first_return_object_build"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::AfterPreFenceMatchEnterBeforeFirstReturnObjectBuild
                );
            },
        );
        with_probe_env(Some("prefence_minimal_stop_return"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::PreFenceMinimalStopReturn);
        });
        with_probe_env(
            Some("prefence_minimal_stop_return_with_cleanup"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::PreFenceMinimalStopReturnWithCleanup
                );
            },
        );
        with_probe_env(Some("before_first_prefence_probe_log"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::BeforeFirstPreFenceProbeLog);
        });
        with_probe_env(
            Some("after_first_prefence_probe_log_before_return"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::AfterFirstPreFenceProbeLogBeforeReturn
                );
            },
        );
        with_probe_env(
            Some("before_create_fence_before_descriptor_pool_destroy"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::BeforeCreateFenceBeforeDescriptorPoolDestroy
                );
            },
        );
        with_probe_env(
            Some("after_descriptor_pool_destroy_before_free_command_buffers"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::AfterDescriptorPoolDestroyBeforeFreeCommandBuffers
                );
            },
        );
        with_probe_env(
            Some("after_free_command_buffers_before_destroy_resource_allocations"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::AfterFreeCommandBuffersBeforeDestroyResourceAllocations
                );
            },
        );
        with_probe_env(
            Some("after_destroy_resource_allocations_before_probe_stop"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::AfterDestroyResourceAllocationsBeforeProbeStop
                );
            },
        );
        with_probe_env(
            Some("after_create_fence_return_before_probe_stop"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::AfterCreateFenceReturnBeforeProbeStop
                );
            },
        );
        with_probe_env(Some("after_upload_semaphore_create"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::AfterUploadSemaphoreCreate);
        });
        with_probe_env(Some("after_compute_semaphore_create"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::AfterComputeSemaphoreCreate);
        });
        with_probe_env(
            Some("at_record_function_entry_before_step_emit"),
            false,
            || {
                let plan = VulkanProbePlan::from_env().expect("probe plan");
                assert_eq!(
                    plan.stage,
                    VulkanProbeStage::AtRecordFunctionEntryBeforeStepEmit
                );
            },
        );
        with_probe_env(Some("before_compute_begin_command_buffer"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(
                plan.stage,
                VulkanProbeStage::BeforeComputeBeginCommandBuffer
            );
        });
        with_probe_env(Some("after_compute_begin_command_buffer"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::AfterComputeBeginCommandBuffer);
        });
        with_probe_env(Some("before_upload_begin_command_buffer"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::BeforeUploadBeginCommandBuffer);
        });
        with_probe_env(Some("after_upload_begin_command_buffer"), false, || {
            let plan = VulkanProbePlan::from_env().expect("probe plan");
            assert_eq!(plan.stage, VulkanProbeStage::AfterUploadBeginCommandBuffer);
        });
    }

    #[tokio::test]
    async fn probe_init_only_stops_without_retaining_submission() {
        let _guard = TEST_GUARD.lock().unwrap();
        let old_stage = env::var("KAIRO_VULKAN_PROBE_STAGE").ok();
        let old_skip_cleanup = env::var("KAIRO_VULKAN_PROBE_SKIP_CLEANUP").ok();
        env::set_var("KAIRO_VULKAN_PROBE_STAGE", "init_only");
        env::remove_var("KAIRO_VULKAN_PROBE_SKIP_CLEANUP");

        let backend = probe_ready_test_backend();
        let handle = backend.submit_batch(VulkanBatchSubmission {
            workload: VulkanWorkloadClass::PacketPreclassification,
            queue: VulkanQueueClass::ComputeOnly,
            payload_len: DEFAULT_PACKET_MIN_BATCH_BYTES,
            surface_words: Some(vec![1, 2, 3, 4]),
            timeout: Duration::from_millis(10),
            requires_zeroize: false,
            allows_gpu: true,
            is_boot_or_recovery_path: false,
            is_truth_boundary: false,
            is_single_block_sync: false,
        });
        let result = backend.wait_for_completion(handle).await;

        match old_stage {
            Some(value) => env::set_var("KAIRO_VULKAN_PROBE_STAGE", value),
            None => env::remove_var("KAIRO_VULKAN_PROBE_STAGE"),
        }
        match old_skip_cleanup {
            Some(value) => env::set_var("KAIRO_VULKAN_PROBE_SKIP_CLEANUP", value),
            None => env::remove_var("KAIRO_VULKAN_PROBE_SKIP_CLEANUP"),
        }

        assert_eq!(
            result.fallback_reason,
            Some(VulkanFallbackReason::ProbeStageStop)
        );
        assert_eq!(result.probe_stage, Some(VulkanProbeStage::InitOnly));
        assert_eq!(
            backend.poll_completion(handle).status,
            VulkanPollStatus::Missing
        );
    }

    #[test]
    fn workload_shader_mapping_tracks_shared_core_split() {
        assert_eq!(
            workload_shader_label(VulkanWorkloadClass::PacketPreclassification),
            "packet_prefilter"
        );
        assert_eq!(
            workload_shader_label(VulkanWorkloadClass::MaintenanceHashing),
            "audit_prefilter"
        );
        assert_eq!(
            workload_shader_label(VulkanWorkloadClass::AuditScan),
            "audit_prefilter"
        );
    }

    #[test]
    fn capability_probe_uses_real_paths_or_overrides() {
        let _guard = TEST_GUARD.lock().unwrap();
        let detected = probe_capabilities_for_test();
        assert!(detected.transfer_available);
        assert!(detected.compute_available);
        assert!(detected.dedicated_zeroize_supported);
        assert!(
            detected.driver_name.contains("/bin/sh") || detected.driver_name.contains("loader:")
        );
        assert!(
            detected.device_name.contains("/bin/sh")
                || detected.device_name.contains("render-node:")
                || detected.device_name != "unbound"
        );
    }

    #[test]
    fn default_thresholds_prioritize_gpu_for_packet_and_audit_workloads() {
        let config = VulkanBackendConfig::default();
        assert_eq!(
            config.packet_preclassification_min_batch_bytes,
            DEFAULT_PACKET_MIN_BATCH_BYTES
        );
        assert_eq!(
            config.maintenance_hashing_min_batch_bytes,
            DEFAULT_MAINTENANCE_MIN_BATCH_BYTES
        );
        assert_eq!(
            config.audit_scan_min_batch_bytes,
            DEFAULT_AUDIT_MIN_BATCH_BYTES
        );
        assert_eq!(
            config.bulk_prefilter_min_batch_bytes,
            DEFAULT_BULK_MIN_BATCH_BYTES
        );
        assert!(config.packet_preclassification_min_batch_bytes < DEFAULT_MIN_BATCH_BYTES);
        assert!(config.audit_scan_min_batch_bytes < DEFAULT_MIN_BATCH_BYTES);
    }

    #[test]
    fn workload_specific_thresholds_override_global_setting() {
        let _guard = TEST_GUARD.lock().unwrap();
        let global = env::var("KAIRO_VULKAN_MIN_BATCH_BYTES").ok();
        let packet = env::var("KAIRO_VULKAN_PACKET_MIN_BATCH_BYTES").ok();
        let maintenance = env::var("KAIRO_VULKAN_MAINTENANCE_MIN_BATCH_BYTES").ok();
        let audit = env::var("KAIRO_VULKAN_AUDIT_MIN_BATCH_BYTES").ok();
        let bulk = env::var("KAIRO_VULKAN_BULK_MIN_BATCH_BYTES").ok();

        env::set_var("KAIRO_VULKAN_MIN_BATCH_BYTES", "131072");
        env::set_var("KAIRO_VULKAN_PACKET_MIN_BATCH_BYTES", "24576");
        env::set_var("KAIRO_VULKAN_MAINTENANCE_MIN_BATCH_BYTES", "98304");
        env::set_var("KAIRO_VULKAN_AUDIT_MIN_BATCH_BYTES", "8192");
        env::remove_var("KAIRO_VULKAN_BULK_MIN_BATCH_BYTES");

        let config = VulkanBackendConfig::default();
        assert_eq!(config.packet_preclassification_min_batch_bytes, 24 * 1024);
        assert_eq!(config.maintenance_hashing_min_batch_bytes, 96 * 1024);
        assert_eq!(config.audit_scan_min_batch_bytes, 8 * 1024);
        assert_eq!(config.bulk_prefilter_min_batch_bytes, 128 * 1024);

        match global {
            Some(value) => env::set_var("KAIRO_VULKAN_MIN_BATCH_BYTES", value),
            None => env::remove_var("KAIRO_VULKAN_MIN_BATCH_BYTES"),
        }
        match packet {
            Some(value) => env::set_var("KAIRO_VULKAN_PACKET_MIN_BATCH_BYTES", value),
            None => env::remove_var("KAIRO_VULKAN_PACKET_MIN_BATCH_BYTES"),
        }
        match maintenance {
            Some(value) => env::set_var("KAIRO_VULKAN_MAINTENANCE_MIN_BATCH_BYTES", value),
            None => env::remove_var("KAIRO_VULKAN_MAINTENANCE_MIN_BATCH_BYTES"),
        }
        match audit {
            Some(value) => env::set_var("KAIRO_VULKAN_AUDIT_MIN_BATCH_BYTES", value),
            None => env::remove_var("KAIRO_VULKAN_AUDIT_MIN_BATCH_BYTES"),
        }
        match bulk {
            Some(value) => env::set_var("KAIRO_VULKAN_BULK_MIN_BATCH_BYTES", value),
            None => env::remove_var("KAIRO_VULKAN_BULK_MIN_BATCH_BYTES"),
        }
    }

    #[test]
    fn threshold_lookup_is_workload_specific() {
        let config = VulkanBackendConfig {
            enable_vulkan: true,
            packet_preclassification_min_batch_bytes: 32 * 1024,
            maintenance_hashing_min_batch_bytes: 64 * 1024,
            audit_scan_min_batch_bytes: 8 * 1024,
            bulk_prefilter_min_batch_bytes: 96 * 1024,
            submit_timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
        };
        assert_eq!(
            config.min_batch_bytes_for(VulkanWorkloadClass::PacketPreclassification),
            32 * 1024
        );
        assert_eq!(
            config.min_batch_bytes_for(VulkanWorkloadClass::MaintenanceHashing),
            64 * 1024
        );
        assert_eq!(
            config.min_batch_bytes_for(VulkanWorkloadClass::AuditScan),
            8 * 1024
        );
        assert_eq!(
            config.min_batch_bytes_for(VulkanWorkloadClass::BulkPrefilter),
            96 * 1024
        );
    }

    #[test]
    fn pack_surface_words_preserves_input_length() {
        let words = pack_surface_words(b"https://api.openai.com");
        assert!(!words.is_empty());
        let reconstructed = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take("https://api.openai.com".len())
            .collect::<Vec<_>>();
        assert_eq!(reconstructed, b"https://api.openai.com");
    }

    #[test]
    fn workload_observation_contract_is_disjoint() {
        assert_eq!(
            observation_slots_for_workload(VulkanWorkloadClass::PacketPreclassification),
            (true, false)
        );
        assert_eq!(
            observation_slots_for_workload(VulkanWorkloadClass::MaintenanceHashing),
            (false, true)
        );
        assert_eq!(
            observation_slots_for_workload(VulkanWorkloadClass::AuditScan),
            (false, true)
        );
    }

    #[test]
    fn zeroize_bytes_scrubs_transient_host_visible_content() {
        let mut bytes = vec![0x5a; 64];
        zeroize_bytes(&mut bytes);
        assert!(bytes.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn pending_wait_is_bounded_to_reduce_busy_polling() {
        let now = Instant::now();
        assert_eq!(
            suggested_pending_wait(
                now,
                now + Duration::from_millis(25),
                now + Duration::from_millis(250),
            ),
            Duration::from_millis(MAX_PENDING_POLL_MS)
        );
        assert_eq!(
            suggested_pending_wait(
                now,
                now + Duration::from_micros(100),
                now + Duration::from_millis(1),
            ),
            Duration::from_millis(MIN_PENDING_POLL_MS)
        );
    }

    #[test]
    fn cleanup_manifest_covers_staged_timeout_resources() {
        let submission = VulkanGpuSubmission {
            workload: VulkanWorkloadClass::AuditScan,
            fence: vk::Fence::from_raw(1),
            upload_complete_semaphore: Some(vk::Semaphore::from_raw(2)),
            compute_complete_semaphore: Some(vk::Semaphore::from_raw(3)),
            compute_command_buffer: vk::CommandBuffer::from_raw(4),
            transfer_command_buffers: [
                Some(vk::CommandBuffer::from_raw(5)),
                Some(vk::CommandBuffer::from_raw(6)),
            ],
            descriptor_pool: vk::DescriptorPool::from_raw(7),
            queue_mode: VulkanQueueRoutingMode::SplitTransferCompute,
            memory_path: VulkanMemoryPath::DeviceLocalStaged,
            uploaded_bytes: 128,
            downloaded_bytes: 64,
            resources: VulkanSubmissionResources {
                input: VulkanBufferAllocation {
                    buffer: vk::Buffer::from_raw(8),
                    memory: vk::DeviceMemory::from_raw(9),
                    allocation_size: 128,
                    host_visible: false,
                },
                output: VulkanBufferAllocation {
                    buffer: vk::Buffer::from_raw(10),
                    memory: vk::DeviceMemory::from_raw(11),
                    allocation_size: 64,
                    host_visible: false,
                },
                input_staging: Some(VulkanBufferAllocation {
                    buffer: vk::Buffer::from_raw(12),
                    memory: vk::DeviceMemory::from_raw(13),
                    allocation_size: 128,
                    host_visible: true,
                }),
                output_staging: Some(VulkanBufferAllocation {
                    buffer: vk::Buffer::from_raw(14),
                    memory: vk::DeviceMemory::from_raw(15),
                    allocation_size: 64,
                    host_visible: true,
                }),
            },
        };
        assert_eq!(
            cleanup_manifest(submission),
            VulkanCleanupManifest {
                command_buffers: 3,
                semaphores: 2,
                buffers: 4,
                memories: 4,
                descriptor_pools: 1,
                fences: 1,
            }
        );
    }
}
