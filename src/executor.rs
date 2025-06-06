use std::sync::{
  atomic::{AtomicBool, Ordering},
  Arc,
};

use crate::core::{Environment, StepReport};
use color_eyre::eyre;
use tokio::sync::{mpsc, Mutex, Notify};

#[derive(Debug)]
pub enum ExecutorReport {
  Redraw,
  Failure { error: crate::core::StepFatal },
}

pub struct Executor {
  environment: Arc<Mutex<Environment>>,
  running: Arc<AtomicBool>,
  tx: mpsc::UnboundedSender<ExecutorReport>,
  device_array: crate::devices::DeviceArray,
  notify: Arc<Notify>,
}

pub struct ExecutorHandler {
  pub environment: Arc<Mutex<Environment>>,
  pub running: Arc<AtomicBool>,
  pub rx: mpsc::UnboundedReceiver<ExecutorReport>,
  pub notify: Arc<Notify>,
}

impl Executor {
  pub fn new(
    environment: Environment,
    device_array: crate::devices::DeviceArray,
  ) -> (Self, ExecutorHandler) {
    let environment = Arc::new(Mutex::new(environment));
    let running = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::unbounded_channel();
    let notify = Arc::new(Notify::new());
    (
      Executor {
        environment: environment.clone(),
        running: running.clone(),
        tx,
        device_array,
        notify: notify.clone(),
      },
      ExecutorHandler {
        environment,
        running,
        rx,
        notify,
      },
    )
  }

  pub async fn process(mut self) -> eyre::Result<()> {
    let mut guard = None;
    loop {
      if self.running.load(Ordering::Acquire) {
        if guard.is_none() {
          guard = Some(self.environment.lock().await);
        }
        let Some(ref mut env) = guard else {
          unreachable!()
        };

        match crate::core::step(env, &mut self.device_array) {
          Ok(StepReport { redraw, .. }) => {
            if redraw {
              self.tx.send(ExecutorReport::Redraw)?;
            }
          }
          Err(e) => {
            std::mem::drop(guard.take());
            self.running.store(false, Ordering::Release);
            log::warn!("Step fatal/halted {:?}", e);
            self.tx.send(ExecutorReport::Failure { error: e })?;
          }
        }

        tokio::task::yield_now().await;
      } else {
        if guard.is_some() {
          std::mem::drop(guard.take());
        }

        self.notify.notified().await;
      }
    }
  }
}
