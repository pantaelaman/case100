use core::Environment;
use std::{
  fs::File,
  path::PathBuf,
  sync::{
    atomic::{AtomicU16, Ordering},
    Arc,
  },
  thread::JoinHandle,
  time::SystemTime,
};

use color_eyre::eyre::{self, OptionExt};
use devices::DeviceArray;
use executor::ExecutorReport;
use itertools::Itertools;
use rat_ftable::{selection::NoSelection, Table, TableState};
use ratatui::{
  crossterm::event,
  layout::{Constraint, Direction, Layout, Margin},
  style::{Color, Style},
  widgets::{Block, Paragraph, Widget},
  DefaultTerminal, Frame,
};
use ratatui_explorer::{FileExplorer, Theme};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tui_input::{backend::crossterm::EventHandler, Input};

mod core;
mod devices;
mod executor;
mod memtable;

fn setup_logger() -> eyre::Result<()> {
  let colors = fern::colors::ColoredLevelConfig::default();
  fern::Dispatch::new()
    .format(move |out, message, record| {
      out.finish(format_args!(
        "[{} {} {}] {}",
        humantime::format_rfc3339_seconds(SystemTime::now()),
        colors.color(record.level()),
        record.target(),
        message
      ))
    })
    .level(log::LevelFilter::Debug)
    .chain(fern::log_file("output.log")?)
    .apply()?;

  Ok(())
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
  color_eyre::install()?;
  setup_logger()?;

  log::info!("Logging harness setup");

  let terminal = ratatui::init();
  let lcd_device = devices::onboard::LcdDisplayDevice::default();
  let hex_device = devices::onboard::HexDisplayDevice::default();

  let device_refs = TerminalDeviceRefs {
    hex0: hex_device.hex0.clone(),
    hex1: hex_device.hex1.clone(),
    lcd_display: lcd_device.lcd.clone(),
  };

  let mut device_array = DeviceArray::default();
  device_array.register_device(Box::new(lcd_device));
  device_array.register_device(Box::new(hex_device));
  let (exec, executor_handler) =
    executor::Executor::new(Environment::default(), device_array);

  let _exec_runner = tokio::spawn(exec.process());
  let result = run(terminal, executor_handler, device_refs).await;

  ratatui::restore();

  result
}

#[derive(Debug, Clone, Copy)]
enum MenuState {
  Normal,
  FileSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuActive {
  Assemble,
  File,
  Load,
  Run,
  Reset,
  Steps,
  Break,
  Watch,
}

impl MenuActive {
  fn decr(self) -> Self {
    match self {
      Self::Assemble => Self::Watch,
      Self::File => Self::Assemble,
      Self::Load => Self::File,
      Self::Run => Self::Load,
      Self::Reset => Self::Run,
      Self::Steps => Self::Reset,
      Self::Break => Self::Steps,
      Self::Watch => Self::Break,
    }
  }

  fn incr(self) -> Self {
    match self {
      Self::Assemble => Self::File,
      Self::File => Self::Load,
      Self::Load => Self::Run,
      Self::Run => Self::Reset,
      Self::Reset => Self::Steps,
      Self::Steps => Self::Break,
      Self::Break => Self::Watch,
      Self::Watch => Self::Assemble,
    }
  }
}

struct TerminalDeviceRefs {
  hex0: Arc<AtomicU16>,
  hex1: Arc<AtomicU16>,
  lcd_display: Arc<Mutex<[[char; 14]; 2]>>,
}

async fn run(
  mut terminal: DefaultTerminal,
  mut executor_handler: executor::ExecutorHandler,
  device_refs: TerminalDeviceRefs,
) -> eyre::Result<()> {
  let theme = Theme::default();
  let mut file_explorer = FileExplorer::with_theme(theme)?;
  let mut state = MenuState::Normal;
  let mut filepath = PathBuf::new();
  let mut active = MenuActive::Assemble;
  let mut steps_input = tui_input::Input::default();
  let mut break_input = tui_input::Input::default();
  let mut watch_input = tui_input::Input::default();
  let mut assembled_environment = Environment::default();
  let mut environment = Environment::default();
  let mut memtable_state = TableState::new();
  let mut request_redraw = true;
  let mut term_event_stream = std::pin::pin! {async_stream::stream! {
    loop {
      yield event::read();
    }
  }};

  let result = loop {
    match state {
      MenuState::Normal => {
        if request_redraw {
          let lcd_text = device_refs
            .lcd_display
            .lock()
            .await
            .iter()
            .map(|line| line.iter().collect::<String>())
            .join("\n");
          terminal.draw(|f| {
            let major_layout = Layout::default()
              .direction(Direction::Vertical)
              .constraints(vec![Constraint::Length(12), Constraint::Fill(1)])
              .split(f.area());

            let control_block = Block::bordered();
            let control_area = control_block.inner(major_layout[0]);
            f.render_widget(control_block, major_layout[0]);

            let control_layout = Layout::default()
              .direction(Direction::Vertical)
              .constraints(vec![
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(4),
              ])
              .split(control_area);

            let top_layout = Layout::default()
              .direction(Direction::Horizontal)
              .constraints(vec![Constraint::Length(16), Constraint::Fill(1)])
              .split(control_layout[0]);

            f.render_widget(
              make_button("Assemble", "[a]", &active, MenuActive::Assemble),
              top_layout[0],
            );

            f.render_widget(
              make_button(
                filepath.file_name().and_then(|f| f.to_str()).unwrap_or(""),
                "MIF File [f]",
                &active,
                MenuActive::File,
              ),
              top_layout[1],
            );

            let middle_layout = Layout::default()
              .direction(Direction::Horizontal)
              .constraints(vec![
                Constraint::Length(16),
                Constraint::Length(16),
                Constraint::Length(16),
                Constraint::Length(16),
                Constraint::Length(16),
                Constraint::Length(16),
                Constraint::Length(6),
                Constraint::Fill(1),
              ])
              .split(control_layout[1]);

            f.render_widget(
              make_button("Load", "[l]", &active, MenuActive::Load),
              middle_layout[0],
            );

            f.render_widget(
              make_button(
                if executor_handler.running.load(Ordering::Relaxed) {
                  "Stop"
                } else {
                  "Run"
                },
                "[r]",
                &active,
                MenuActive::Run,
              ),
              middle_layout[1],
            );

            f.render_widget(
              make_button("Reset", "[ESC]", &active, MenuActive::Reset),
              middle_layout[2],
            );

            f.render_widget(
              make_button(
                steps_input.value(),
                "Steps",
                &active,
                MenuActive::Steps,
              ),
              middle_layout[3],
            );
            f.render_widget(
              make_button(
                break_input.value(),
                "Break",
                &active,
                MenuActive::Break,
              ),
              middle_layout[4],
            );
            f.render_widget(
              make_button(
                watch_input.value(),
                "Watch",
                &active,
                MenuActive::Watch,
              ),
              middle_layout[5],
            );

            f.render_widget(
              Paragraph::new("").block(Block::bordered().title("IAR")),
              middle_layout[6],
            );

            f.render_widget(
              Paragraph::new(
                if executor_handler.running.load(Ordering::Relaxed) {
                  "Running"
                } else {
                  "Stopped"
                },
              ),
              middle_layout[7],
            );

            let hex_lcd_layout = Layout::default()
              .direction(Direction::Horizontal)
              .constraints(vec![
                Constraint::Length(6),
                Constraint::Length(6),
                Constraint::Length(18),
                Constraint::Fill(1),
              ])
              .split(control_layout[2]);

            f.render_widget(
              Paragraph::new(format!(
                "{:04x}",
                device_refs.hex1.load(Ordering::Relaxed)
              ))
              .block(Block::bordered().title("H7-4")),
              hex_lcd_layout[0],
            );
            f.render_widget(
              Paragraph::new(format!(
                "{:04x}",
                device_refs.hex0.load(Ordering::Relaxed)
              ))
              .block(Block::bordered().title("H3-0")),
              hex_lcd_layout[1],
            );
            f.render_widget(
              Paragraph::new(lcd_text).block(Block::bordered().title("LCD")),
              hex_lcd_layout[2],
            );

            f.render_stateful_widget(
              Table::<NoSelection>::new()
                .data(&environment)
                .widths([Constraint::Fill(1); 11])
                .block(Block::bordered().title("Memory")),
              major_layout[1],
              &mut memtable_state,
            );
          })?;
        }

        request_redraw = true;
        tokio::select! {
          event = term_event_stream.next() => {
            let event = event.ok_or_eyre("Crossterm event pipe empty")??;
            match event {
              event::Event::Key(key) => match key.code {
                event::KeyCode::Char('q') => break Ok(()),
                event::KeyCode::Tab => active = active.incr(),
                event::KeyCode::BackTab => active = active.decr(),
                event::KeyCode::Esc => {
                  executor_handler.running.store(false, Ordering::SeqCst);
                  let mut guard = executor_handler.environment.lock().await;
                  guard.iar = 0;
                  environment = guard.clone();
                  std::mem::drop(guard);
                },
                event::KeyCode::Char(c) => {
                  if c.is_digit(10) {
                    match active {
                      MenuActive::Steps => {
                        steps_input.handle_event(&event);
                      }
                      MenuActive::Break => {
                        break_input.handle_event(&event);
                      }
                      MenuActive::Watch => {
                        watch_input.handle_event(&event);
                      }
                      _ => {
                        request_redraw = false;
                      }
                    }
                  } else {
                    match c {
                      'a' => {
                        active = MenuActive::Assemble;
                        assembled_environment =
                          Environment::parse(&mut File::open(filepath.clone())?)?;
                      }
                      'r' => {
                        active = MenuActive::Run;
                        if executor_handler.running.load(Ordering::Acquire) {
                          executor_handler.running.store(false, Ordering::SeqCst);
                          let guard = executor_handler.environment.lock().await;
                          environment = guard.clone();
                          std::mem::drop(guard);
                        } else {
                          executor_handler.running.store(true, Ordering::Release);
                        }
                      }
                      'l' => {
                        environment = assembled_environment.clone();
                        active = MenuActive::Load;
                        executor_handler.running.store(false, Ordering::SeqCst);
                        log::debug!("awaiting stoppage of executor");
                        let mut guard = executor_handler.environment.lock().await;
                        log::debug!("executor stopped successfully, lock acquired");
                        *guard = environment.clone();
                        std::mem::drop(guard);
                      }
                      'f' => {
                        state = MenuState::FileSelection;
                        active = MenuActive::File;
                      }
                      _ => {
                        request_redraw = false;
                      }
                    }
                  }
                }
                _ => {
                  request_redraw = false;
                }
              },
              event::Event::Resize(_, _) => {}
              _ => {
                request_redraw = false;
              }
            }
          },
          Some(report) = executor_handler.rx.recv() => {
            match report {
              ExecutorReport::Failure { error } => {
                log::warn!("Received failure report {:?}", error);
                let guard = executor_handler.environment.lock().await;
                environment = guard.clone();
                std::mem::drop(guard);
              },
              ExecutorReport::DeviceUpdate => {
                log::info!("Redrawing due to device update");
              },
            }
          }
        };
      }
      MenuState::FileSelection => {
        terminal.draw(|f| {
          f.render_widget(&file_explorer.widget(), f.area());
        })?;

        let event = term_event_stream
          .next()
          .await
          .ok_or_eyre("Crossterm event pipe disconnected")??;
        if let event::Event::Key(key) = event {
          match key.code {
            event::KeyCode::Char('q') => break Ok(()),
            event::KeyCode::Esc => {
              state = MenuState::Normal;
              continue;
            }
            event::KeyCode::Enter => {
              filepath = file_explorer.current().path().to_owned();
              state = MenuState::Normal;
              continue;
            }
            _ => {}
          }
        }

        file_explorer.handle(&event)?;
      }
    }
  };

  executor_handler.running.store(false, Ordering::SeqCst);
  log::info!("Acquiring guard for death");
  let _ = executor_handler.environment.lock().await;
  log::info!("Death guard acquired");

  result
}

fn make_button<'a>(
  text: &'a str,
  title: &'a str,
  active: &'a MenuActive,
  target: MenuActive,
) -> impl Widget + use<'a> {
  Paragraph::new(text).block(
    Block::bordered()
      .title(title)
      .style(Style::new().fg(get_colour(active, target))),
  )
}

fn get_colour(active: &MenuActive, target: MenuActive) -> Color {
  if active == &target {
    Color::Green
  } else {
    Color::Blue
  }
}
