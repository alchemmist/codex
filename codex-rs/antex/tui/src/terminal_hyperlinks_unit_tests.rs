use super::*;
use pretty_assertions::assert_eq;
use ratatui::style::Style;

#[test]
fn only_web_destinations_receive_osc8() {
    assert!(osc8_hyperlink("https://example.com/a", "a").contains("\x1b]8;;"));
    assert_eq!(osc8_hyperlink("mailto:a@example.com", "a"), "a");
    assert_eq!(
        osc8_hyperlink("https://example.com/\u{7}safe", "a"),
        "\x1b]8;;https://example.com/safe\x07a\x1b]8;;\x07"
    );
    assert_eq!(
        strip_osc8(&osc8_hyperlink("https://example.com/a", "visible")),
        "visible"
    );
}

#[test]
fn discovers_punctuated_web_url_columns() {
    assert_eq!(
        web_links_in_text("See (https://example.com/a)."),
        vec![TerminalHyperlink::web(
            /*columns*/ 5..26,
            "https://example.com/a".to_string(),
        )]
    );
}

#[test]
fn hyperlink_columns_follow_a_long_prefix_without_wrapping() {
    let prefix = "a".repeat(65_536);
    let destination = "https://example.com/long-prefix";
    let text = format!("{prefix} {destination}");

    assert_eq!(
        HyperlinkLine::new(Line::from(text.clone())).width(),
        text.len()
    );
    assert_eq!(
        web_links_in_text(&text),
        vec![TerminalHyperlink::web(
            /*columns*/ 65_537..65_537 + destination.len(),
            destination.to_string(),
        )]
    );
}

#[test]
fn preserves_balanced_parentheses_in_bare_web_urls() {
    let destination = "https://en.wikipedia.org/wiki/Function_(mathematics)";
    assert_eq!(
        web_links_in_text(&format!("See ({destination}).")),
        vec![TerminalHyperlink::web(
            /*columns*/ 5..5 + usize::from(destination.cell_width()),
            destination.to_string(),
        )]
    );
}

#[test]
fn decorates_a_contiguous_web_link_with_one_osc8_pair() {
    let destination = "https://example.com/a/very/long/path";
    let line = HyperlinkLine {
        line: Line::from(destination),
        hyperlinks: vec![TerminalHyperlink::web(
            /*columns*/ 0..usize::from(destination.cell_width()),
            destination.to_string(),
        )],
    };

    assert_eq!(
        decorate_spans(&line),
        vec![Span::from(osc8_hyperlink(destination, destination))]
    );
    assert_eq!(
        decorate_spans(&HyperlinkLine::new(Line::from("not linked"))),
        vec![Span::from("not linked")]
    );
}

#[test]
fn wrapping_maps_repeated_link_labels_by_source_position() {
    let mut source = HyperlinkLine::new(Line::from("here here"));
    source.hyperlinks.push(TerminalHyperlink::web(
        /*columns*/ 5..9,
        "https://example.com".to_string(),
    ));

    let wrapped = remap_wrapped_line(&source, vec![Line::from("here here")]);

    assert_eq!(
        wrapped[0].hyperlinks,
        vec![TerminalHyperlink::web(
            /*columns*/ 5..9,
            "https://example.com".to_string(),
        )]
    );
}

#[test]
fn wrapping_maps_multiple_links_across_indented_unicode_lines() {
    let text = "alpha 😀here middle there end";
    let first_start = text.find("here").expect("first link");
    let second_start = text.find("there").expect("second link");
    let first_column = usize::from(text[..first_start].cell_width());
    let second_column = usize::from(text[..second_start].cell_width());
    let mut source = HyperlinkLine::new(Line::from(text));
    source.hyperlinks.push(TerminalHyperlink::web(
        first_column..first_column + usize::from("here".cell_width()),
        "https://example.com/first".to_string(),
    ));
    source.hyperlinks.push(TerminalHyperlink::web(
        second_column..second_column + usize::from("there".cell_width()),
        "https://example.com/second".to_string(),
    ));

    let wrapped = remap_wrapped_line(
        &source,
        vec![
            Line::from("  alpha 😀here"),
            Line::from("    middle there end"),
        ],
    );

    assert_eq!(
        wrapped,
        vec![
            HyperlinkLine {
                line: Line::from("  alpha 😀here"),
                hyperlinks: vec![TerminalHyperlink::web(
                    /*columns*/ 10..14,
                    "https://example.com/first".to_string(),
                )],
            },
            HyperlinkLine {
                line: Line::from("    middle there end"),
                hyperlinks: vec![TerminalHyperlink::web(
                    /*columns*/ 11..16,
                    "https://example.com/second".to_string(),
                )],
            },
        ]
    );
}

#[test]
fn buffer_hyperlinks_follow_word_wrapping() {
    let destination = "https://example.com/path";
    let mut line = HyperlinkLine::new(Line::from(format!("See {destination} now")));
    line.hyperlinks.push(TerminalHyperlink::web(
        /*columns*/ 4..4 + usize::from(destination.cell_width()),
        destination.to_string(),
    ));
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 18, /*height*/ 4,
    );
    let mut buf = Buffer::empty(area);

    HyperlinkParagraph::new(&[line], Style::default()).render(area, &mut buf);

    let linked_text = area
        .positions()
        .filter_map(|position| {
            let symbol = buf[position].symbol();
            symbol
                .contains(&format!("\x1b]8;;{destination}\x07"))
                .then(|| strip_osc8(symbol))
        })
        .collect::<String>();
    assert_eq!(linked_text, destination);
}

#[test]
fn buffer_hyperlinks_follow_scrolled_wrapped_rows() {
    let hidden_destination = "https://example.com/hidden";
    let visible_destination = "https://example.com/visible";
    let trailing_destination = "https://example.com/trailing";

    let mut hidden = HyperlinkLine::new(Line::default());
    hidden.push_span("hidden".into(), Some(hidden_destination));
    let mut visible = HyperlinkLine::new(Line::from("prefix "));
    visible.push_span("visible-link".into(), Some(visible_destination));
    let mut trailing = HyperlinkLine::new(Line::default());
    trailing.push_span("trailing".into(), Some(trailing_destination));
    let lines = vec![hidden, visible, trailing];

    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 8, /*height*/ 2,
    );
    let backend = crate::test_backend::VT100Backend::new(area.width, area.height);
    let mut terminal = crate::custom_terminal::Terminal::with_options(backend).expect("terminal");
    terminal.set_viewport_area(area);
    terminal
        .draw(|frame| {
            let buf = frame.buffer_mut();
            HyperlinkParagraph::new(&lines, Style::default())
                .scroll(/*rows*/ 2)
                .render(area, buf);

            let linked_text = area
                .positions()
                .filter_map(|position| {
                    let symbol = buf[position].symbol();
                    symbol
                        .contains(&format!("\x1b]8;;{visible_destination}\x07"))
                        .then(|| strip_osc8(symbol))
                })
                .collect::<String>();
            assert_eq!(linked_text, "visible-link");
        })
        .expect("render scrolled hyperlinks");

    insta::assert_snapshot!(
        "buffer_hyperlinks_follow_scrolled_wrapped_rows",
        terminal.backend()
    );
}

#[test]
fn buffer_hyperlinks_follow_wrapped_wide_glyphs() {
    let destination = "https://example.com/wide";
    let mut line = HyperlinkLine::new(Line::from("前文 "));
    line.push_span("漢字漢字".into(), Some(destination));
    line.push_span(" 後文".into(), /*destination*/ None);
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 6, /*height*/ 4,
    );
    let mut buf = Buffer::empty(area);

    Paragraph::new(Text::from(line.line.clone()))
        .wrap(Wrap { trim: false })
        .render(area, &mut buf);
    mark_buffer_hyperlinks(&mut buf, area, &[line], /*scroll_rows*/ 0);

    let linked_text = area
        .positions()
        .filter_map(|position| {
            let symbol = buf[position].symbol();
            symbol
                .contains(&format!("\x1b]8;;{destination}\x07"))
                .then(|| strip_osc8(symbol))
        })
        .collect::<String>();
    assert_eq!(linked_text, "漢字漢字");
}

#[test]
fn buffer_hyperlinks_follow_wrapped_halfwidth_dakuten() {
    let destination = "https://example.com/dakuten";
    let mut line = HyperlinkLine::new(Line::from("ｶﾞ "));
    line.push_span("ﾊﾟlink".into(), Some(destination));
    line.push_span(" tail".into(), /*destination*/ None);
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 5, /*height*/ 4,
    );
    let mut buf = Buffer::empty(area);

    Paragraph::new(Text::from(line.line.clone()))
        .wrap(Wrap { trim: false })
        .render(area, &mut buf);
    mark_buffer_hyperlinks(&mut buf, area, &[line], /*scroll_rows*/ 0);

    let linked_text = area
        .positions()
        .filter_map(|position| {
            let symbol = buf[position].symbol();
            symbol
                .contains(&format!("\x1b]8;;{destination}\x07"))
                .then(|| strip_osc8(symbol))
        })
        .collect::<String>();
    assert_eq!(linked_text, "ﾊﾟlink");
}

#[test]
fn forced_width_hyperlinks_render_wide_and_halfwidth_cells_snapshot() {
    let destination = "https://example.com/rendered";
    let mut line = HyperlinkLine::new(Line::from("prefix "));
    line.push_span("漢字 ｶﾞ".into(), Some(destination));
    line.push_span(" tail".into(), /*destination*/ None);

    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 14, /*height*/ 3,
    );
    let backend = crate::test_backend::VT100Backend::new(area.width, area.height);
    let mut terminal = crate::custom_terminal::Terminal::with_options(backend).expect("terminal");
    terminal.set_viewport_area(area);

    terminal
        .draw(|frame| {
            Paragraph::new(Text::from(line.line.clone()))
                .wrap(Wrap { trim: false })
                .render(area, frame.buffer_mut());
            mark_buffer_hyperlinks(
                frame.buffer_mut(),
                area,
                &[line.clone()],
                /*scroll_rows*/ 0,
            );
        })
        .expect("render hyperlinks");

    insta::assert_snapshot!(
        "forced_width_hyperlinks_render_wide_and_halfwidth_cells",
        terminal.backend()
    );
}

#[test]
fn buffer_hyperlinks_preserve_visible_cell_width_for_ratatui_diff() {
    let destination = "https://example.com/dakuten";
    let mut line = HyperlinkLine::new(Line::from("ｶﾞ tail"));
    line.hyperlinks.push(TerminalHyperlink::web(
        /*columns*/ 0..2,
        destination.to_string(),
    ));
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 7, /*height*/ 1,
    );
    let previous = Buffer::with_lines(["       "]);
    let mut next = Buffer::empty(area);

    Paragraph::new(Text::from(line.line.clone())).render(area, &mut next);
    mark_buffer_hyperlinks(&mut next, area, &[line], /*scroll_rows*/ 0);

    assert_eq!(next[(0, 0)].cell_width(), 2);
    assert!(matches!(
        next[(0, 0)].diff_option,
        CellDiffOption::ForcedWidth(width) if width.get() == 2
    ));
    assert_eq!(
        previous
            .diff_iter(&next)
            .map(|(x, _, cell)| (x, strip_osc8(cell.symbol())))
            .collect::<Vec<_>>(),
        vec![
            (0, "ｶﾞ".to_string()),
            (3, "t".to_string()),
            (4, "a".to_string()),
            (5, "i".to_string()),
            (6, "l".to_string()),
        ]
    );
}

#[test]
fn matching_hyperlinks_preserve_visible_cell_width_for_ratatui_diff() {
    let destination = "https://example.com/dakuten";
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 7, /*height*/ 1,
    );
    let previous = Buffer::with_lines(["       "]);
    let mut next = Buffer::empty(area);
    next.set_string(
        /*x*/ 0,
        /*y*/ 0,
        "ｶﾞ tail",
        Style::default().add_modifier(Modifier::UNDERLINED),
    );

    mark_underlined_hyperlink(&mut next, area, destination);

    assert_eq!(next[(0, 0)].cell_width(), 2);
    assert!(matches!(
        next[(0, 0)].diff_option,
        CellDiffOption::ForcedWidth(width) if width.get() == 2
    ));
    assert_eq!(
        previous
            .diff_iter(&next)
            .map(|(x, _, cell)| (x, strip_osc8(cell.symbol())))
            .collect::<Vec<_>>(),
        vec![
            (0, "ｶﾞ".to_string()),
            (2, " ".to_string()),
            (3, "t".to_string()),
            (4, "a".to_string()),
            (5, "i".to_string()),
            (6, "l".to_string()),
        ]
    );
}

#[test]
fn trusted_file_destination_receives_osc8_without_enabling_plain_file_links() {
    let temp_dir = tempfile::tempdir().expect("temp directory");
    let file_url = Url::from_file_path(temp_dir.path().join("viewer.html"))
        .expect("test path should convert to file URL");
    let mut link = TerminalHyperlink::web(
        /*columns*/ 0..4,
        "https://codex.invalid/viewer".to_string(),
    );
    link.retarget_to_trusted_file(&file_url);
    let line = HyperlinkLine {
        line: Line::from("view"),
        hyperlinks: vec![link],
    };

    assert_eq!(
        decorate_spans(&line),
        vec![Span::from(format!(
            "\x1b]8;;{file_url}\x07view\x1b]8;;\x07"
        ))]
    );
    assert_eq!(osc8_hyperlink(file_url.as_str(), "view"), "view");
}
