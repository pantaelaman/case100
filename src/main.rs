use core::Environment;
use std::{fs::File, path::PathBuf};

use color_eyre::eyre;
use ratatui::{
  crossterm::event,
  layout::{Constraint, Direction, Layout, Margin},
  style::{Color, Style},
  widgets::{Block, Paragraph, Widget},
  DefaultTerminal, Frame,
};
use ratatui_explorer::{FileExplorer, Theme};
use tui_input::{backend::crossterm::EventHandler, Input};

mod core;
mod devices;
mod executor;

#[tokio::main]
async fn main() -> eyre::Result<()> {
  color_eyre::install()?;

  let terminal = ratatui::init();
  let result = run(terminal);
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

fn run(mut terminal: DefaultTerminal) -> eyre::Result<()> {
  let theme = Theme::default();
  let mut file_explorer = FileExplorer::with_theme(theme)?;
  let mut state = MenuState::Normal;
  let mut filepath = PathBuf::new();
  let mut active = MenuActive::Assemble;
  let mut steps_input = tui_input::Input::default();
  let mut break_input = tui_input::Input::default();
  let mut watch_input = tui_input::Input::default();
  let mut environment = Environment::default();

  loop {
    match state {
      MenuState::Normal => {
        terminal.draw(|f| {
          let major_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(12), Constraint::Fill(1)])
            .split(f.area());

          let buf = f.buffer_mut();

          let control_block = Block::bordered();
          let control_area = control_block.inner(major_layout[0]);
          control_block.render(major_layout[0], buf);

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

          make_button("Assemble", "[a]", &active, MenuActive::Assemble)
            .render(top_layout[0], buf);

          make_button(
            filepath.file_name().and_then(|f| f.to_str()).unwrap_or(""),
            "MIF File [f]",
            &active,
            MenuActive::File,
          )
          .render(top_layout[1], buf);

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

          make_button("Load", "[l]", &active, MenuActive::Load)
            .render(middle_layout[0], buf);

          make_button("Run", "[r]", &active, MenuActive::Run)
            .render(middle_layout[1], buf);

          make_button("Reset", "[ESC]", &active, MenuActive::Reset)
            .render(middle_layout[2], buf);

          make_button(steps_input.value(), "Steps", &active, MenuActive::Steps)
            .render(middle_layout[3], buf);
          make_button(break_input.value(), "Break", &active, MenuActive::Break)
            .render(middle_layout[4], buf);
          make_button(watch_input.value(), "Watch", &active, MenuActive::Watch)
            .render(middle_layout[5], buf);

          let hex_lcd_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
              Constraint::Length(6),
              Constraint::Length(6),
              Constraint::Length(18),
              Constraint::Fill(1),
            ])
            .split(control_layout[2]);

          Paragraph::new("")
            .block(Block::bordered().title("H7-4"))
            .render(hex_lcd_layout[0], buf);
          Paragraph::new("")
            .block(Block::bordered().title("H3-0"))
            .render(hex_lcd_layout[1], buf);

          Paragraph::new("")
            .block(Block::bordered().title("LCD"))
            .render(hex_lcd_layout[2], buf);
        })?;

        let event = event::read()?;
        if let event::Event::Key(key) = event {
          match key.code {
            event::KeyCode::Char('q') => break Ok(()),
            event::KeyCode::Tab => active = active.incr(),
            event::KeyCode::BackTab => active = active.decr(),
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
                  _ => {}
                }
              } else {
                match c {
                  'a' => {
                    active = MenuActive::Assemble;
                    environment =
                      Environment::parse(&mut File::open(filepath.clone())?)?;
                  }
                  'f' => {
                    state = MenuState::FileSelection;
                    active = MenuActive::File;
                  }
                  _ => {}
                }
              }
            }
            _ => {}
          }
        }
      }
      MenuState::FileSelection => {
        terminal.draw(|f| {
          f.render_widget(&file_explorer.widget(), f.area());
        })?;

        let event = event::read()?;
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
  }
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
