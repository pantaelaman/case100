use rat_ftable::TableData;
use ratatui::widgets::{Paragraph, Widget};

use crate::core::Environment;

impl<'a> TableData<'a> for &'a Environment {
  fn rows(&self) -> usize {
    (crate::core::MEMORY_SIZE / 10) + 1
  }

  fn render_cell(
    &self,
    ctx: &rat_ftable::TableContext,
    column: usize,
    row: usize,
    area: ratatui::prelude::Rect,
    buf: &mut ratatui::prelude::Buffer,
  ) {
    // left side is addresses
    if column == 0 {
      Paragraph::new(format!("{} ", row * 10))
        .style(ctx.style)
        .right_aligned()
        .render(area, buf);
    } else {
      Paragraph::new(format!("{}", self.memory[row * 10 + column - 1]))
        .style(ctx.style)
        .left_aligned()
        .render(area, buf);
    }
  }
}
