use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use color_eyre::eyre;
use tokio::sync::{watch, Mutex, Notify};

use crate::sdlcore::SdlKbdEvent;

use super::{DeviceError, DeviceFrame};

pub struct KbdDevice {
  turn: Arc<AtomicBool>,
  notify: Arc<Notify>,
  event: Arc<Mutex<SdlKbdEvent>>,
}

impl KbdDevice {
  pub fn init(mut kbd_ev_rx: watch::Receiver<SdlKbdEvent>) -> Self {
    let event = Arc::new(Mutex::new(*kbd_ev_rx.borrow()));
    let notify = Arc::new(Notify::new());
    let turn = Arc::new(AtomicBool::new(false));

    let remote_event_handle = event.clone();
    let remote_notify = notify.clone();
    let remote_turn = turn.clone();
    tokio::spawn(async move {
      loop {
        tracing::info!("kbd awaiting notification");
        remote_notify.notified().await;
        tracing::info!("kbd received notification");
        let mut guard = remote_event_handle.lock().await;
        kbd_ev_rx.changed().await?;
        *guard = kbd_ev_rx.borrow().clone();
        std::mem::drop(guard);
        remote_turn.store(false, Ordering::SeqCst);
      }

      #[allow(unreachable_code)]
      eyre::Result::<()>::Ok(())
    });

    KbdDevice {
      notify,
      event,
      turn,
    }
  }
}

impl DeviceFrame for KbdDevice {
  fn registers(&self) -> &'static [u32] {
    &[0x80000020, 0x80000021, 0x80000022]
  }

  fn set(
    &mut self,
    register: u32,
    value: i32,
  ) -> Result<bool, super::DeviceError> {
    if self.turn.load(Ordering::SeqCst) {
      Err(DeviceError::Busy)
    } else if register != 0x80000020 || value != 1 {
      Err(DeviceError::Unwritable)
    } else {
      self.turn.store(true, Ordering::SeqCst);
      self.notify.notify_waiters();
      Ok(false)
    }
  }

  fn get(&mut self, register: u32) -> Result<i32, DeviceError> {
    if register == 0x80000020 {
      return Ok(self.turn.load(Ordering::Relaxed) as i32);
    } else if self.turn.load(Ordering::Acquire) {
      return Err(DeviceError::Busy);
    }

    let guard = self.event.try_lock().map_err(|_| DeviceError::Busy)?;
    match register {
      0x80000021 => Ok(guard.down as i32),
      0x80000022 => Ok(guard.keycode as i32),
      _ => unreachable!(),
    }
  }
}
