use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc,
};

use resolution_hub_lib::{
    model::{PendingRestore, RuntimeStatus},
    profile,
    runtime::RuntimeManager,
};

pub struct Snapshot {
    pub runtime: RuntimeStatus,
    pub pending: Option<Result<Option<PendingRestore>, String>>,
    revision: u64,
}

type Completion = Box<dyn FnOnce(Snapshot) + Send>;

/// 运行状态锁和恢复文件都只在此线程读取；没有请求时阻塞等待，不轮询磁盘。
#[derive(Clone)]
pub struct Reader {
    sender: mpsc::Sender<Completion>,
    revision: Arc<AtomicU64>,
}

impl Reader {
    pub fn start(manager: RuntimeManager) -> Result<Self, std::io::Error> {
        Self::with_load(move || {
            let _timing = crate::performance::measure("runtime-read");
            let runtime = manager.status();
            let pending =
                matches!(runtime.phase.as_str(), "idle" | "error").then(profile::load_pending);
            Snapshot {
                runtime,
                pending,
                revision: 0,
            }
        })
    }

    fn with_load(
        mut load: impl FnMut() -> Snapshot + Send + 'static,
    ) -> Result<Self, std::io::Error> {
        let (sender, receiver) = mpsc::channel::<Completion>();
        let revision = Arc::new(AtomicU64::new(0));
        let worker_revision = revision.clone();
        std::thread::Builder::new()
            .name("native-runtime-reader".into())
            .spawn(move || {
                while let Ok(complete) = receiver.recv() {
                    let snapshot = loop {
                        let version = worker_revision.load(Ordering::Acquire);
                        let mut snapshot = load();
                        if worker_revision.load(Ordering::Acquire) == version {
                            snapshot.revision = version;
                            break snapshot;
                        }
                    };
                    complete(snapshot);
                }
            })?;
        Ok(Self { sender, revision })
    }

    pub fn invalidate(&self) {
        self.revision.fetch_add(1, Ordering::AcqRel);
    }

    pub fn is_current(&self, snapshot: &Snapshot) -> bool {
        snapshot.revision == self.revision.load(Ordering::Acquire)
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    pub fn read(&self, complete: impl FnOnce(Snapshot) + Send + 'static) -> Result<(), String> {
        self.sender
            .send(Box::new(complete))
            .map_err(|_| "无法读取运行与桌面恢复状态，请重新打开应用".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    fn snapshot() -> Snapshot {
        Snapshot {
            runtime: RuntimeStatus::default(),
            pending: Some(Err("恢复点读取失败".into())),
            revision: 0,
        }
    }

    #[test]
    fn slow_reads_run_off_the_caller_thread_and_keep_recovery_errors() {
        let caller = thread::current().id();
        let reader = Reader::with_load(move || {
            assert_ne!(caller, thread::current().id());
            snapshot()
        })
        .unwrap();
        let (sender, receiver) = mpsc::channel();
        reader
            .read(move |snapshot| sender.send(snapshot).unwrap())
            .unwrap();
        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(reader.is_current(&snapshot));
        assert_eq!(snapshot.pending.unwrap().unwrap_err(), "恢复点读取失败");
    }

    #[test]
    fn a_runtime_change_during_a_read_retries_and_invalidates_queued_results() {
        let (started, started_receiver) = mpsc::channel();
        let (resume, resume_receiver) = mpsc::channel();
        let mut reads = 0;
        let reader = Reader::with_load(move || {
            reads += 1;
            if reads == 1 {
                started.send(()).unwrap();
                resume_receiver
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
            }
            let mut result = snapshot();
            result.runtime.phase = if reads == 1 { "running" } else { "idle" }.into();
            result
        })
        .unwrap();
        let (sender, receiver) = mpsc::channel();
        reader
            .read(move |snapshot| sender.send(snapshot).unwrap())
            .unwrap();
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        reader.invalidate();
        resume.send(()).unwrap();
        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(snapshot.runtime.phase, "idle");
        assert!(reader.is_current(&snapshot));
        reader.invalidate();
        assert!(!reader.is_current(&snapshot));
    }
}
