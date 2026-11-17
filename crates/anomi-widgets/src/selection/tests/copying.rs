//! Dragging across rows copies them in reading order: names and times
//! level with each other on a line, pictures as `[Picture]`, rows scrolled
//! out of the built window whole, and rows that went in above since leave
//! it where it was.

use iced::{Point, mouse};

use super::*;

#[test]
fn a_selection_runs_across_messages_with_their_names_and_times() {
    let chat = Chat::new(
        20,
        &[
            ("Sam", "Thinking **co-op** after nine."),
            ("Riley", "Yep. I should be free after dinner."),
        ],
    );
    let mut ui = simulate(chat.view(front()));
    // From the end of the lowest message up past every row.
    let lowest = bounds(&mut ui, "Yep. I should be free after dinner.");
    drag(
        &mut ui,
        end_of(lowest),
        &[Point::new(300.0, lowest.y - 8.0), Point::new(300.0, 4.0)],
    );
    let selected: Vec<Asked> = ui.drain().collect();
    let whole =
        "Sam 19:20\nThinking co-op after nine.\nRiley 19:21\nYep. I should be free after dinner.";
    assert_eq!(
        selected,
        [Asked::Selected(whole.into())],
        "letting go tells it"
    );
    assert_eq!(copy(&mut ui).as_deref(), Some(whole));
}

#[test]
fn a_selection_copies_the_pictures_it_runs_through() {
    let mut chat = Chat::new(20, &[("Sam", "Look at this."), ("Riley", "Nice.")]);
    chat.messages[0].pictures = 1;
    let mut ui = simulate(chat.view(front()));
    let lowest = bounds(&mut ui, "Nice.");
    drag(&mut ui, end_of(lowest), &[Point::new(300.0, 4.0)]);
    assert_eq!(
        copy(&mut ui).as_deref(),
        Some("Sam 19:20\nLook at this.\n[Picture]\nRiley 19:21\nNice.")
    );
}

#[test]
fn a_drag_within_a_message_copies_what_it_covers() {
    let chat = Chat::new(
        20,
        &[(
            "Sam",
            "Thinking **co-op** after nine, if people are up for it.",
        )],
    );
    let mut ui = simulate(chat.view(front()));
    let body = bounds(
        &mut ui,
        "Thinking co-op after nine, if people are up for it.",
    );
    // From right of the text, past its end, to left of the timeline.
    drag(
        &mut ui,
        end_of(body),
        &[Point::new(body.x - 4.0, body.center_y())],
    );
    assert_eq!(
        copy(&mut ui).as_deref(),
        Some("Thinking co-op after nine, if people are up for it.")
    );
}

#[test]
fn a_selection_keeps_the_messages_it_scrolls_past() {
    let chat = Chat::many(40);
    let mut ui = simulate(chat.view(front()));
    let lowest = bounds(&mut ui, "Message 39.");
    move_to(&mut ui, end_of(lowest));
    press(&mut ui, mouse::Button::Left);
    // Past the list's top: it scrolls up, frame by frame, to its first row.
    move_to(&mut ui, Point::new(300.0, -200.0));
    frames(&mut ui, Instant::now(), Duration::from_millis(50), 80);
    assert!(
        ui.find("Message 0.").is_ok(),
        "the list scrolled to its start"
    );
    release(&mut ui, mouse::Button::Left);
    let expected: Vec<String> = (0..40).map(|at| chat.copied(at)).collect();
    assert_eq!(copy(&mut ui), Some(expected.join("\n")));
}

#[test]
fn rows_scrolled_out_of_the_built_window_copy_whole() {
    let chat = Chat::many(40);
    let mut ui = simulate(chat.view(front()));
    let lowest = bounds(&mut ui, "Message 39.");
    move_to(&mut ui, end_of(lowest));
    press(&mut ui, mouse::Button::Left);
    move_to(&mut ui, Point::new(300.0, -200.0));
    frames(&mut ui, Instant::now(), Duration::from_millis(50), 80);
    // The drag ends past the end of the first message: none of it, and
    // every message after it, most of them built only on the way.
    let first = bounds(&mut ui, "Message 0.");
    move_to(&mut ui, end_of(first));
    release(&mut ui, mouse::Button::Left);
    let copied = copy(&mut ui).expect("a selection");
    assert!(copied.starts_with("Sam 19:01\nMessage 1."), "{copied}");
    assert!(copied.ends_with(&chat.copied(39)), "{copied}");
    assert_eq!(copied.lines().count(), 39 * 2);
}

#[test]
fn rows_going_in_above_leave_the_selection_where_it_was() {
    let chat = Chat::new(
        20,
        &[("Sam", "First."), ("Riley", "Second."), ("Sam", "Third.")],
    );
    let older = Chat::new(
        18,
        &[
            ("Alex", "Older."),
            ("Alex", "Oldest."),
            ("Sam", "First."),
            ("Riley", "Second."),
            ("Sam", "Third."),
        ],
    );
    let mut ui = simulate(chat.view(front()));
    let second = bounds(&mut ui, "Second.");
    let third = bounds(&mut ui, "Third.");
    drag(
        &mut ui,
        end_of(third),
        &[Point::new(second.x - 4.0, second.center_y())],
    );
    let before = copy(&mut ui).expect("a selection");
    assert_eq!(before, "Second.\nSam 19:22\nThird.");
    // Two older messages load above.
    let mut ui = ui.rebuild(older.view(front()));
    assert_eq!(copy(&mut ui), Some(before));
}

#[test]
fn another_rooms_rows_drop_the_selection() {
    let chat = Chat::new(20, &[("Sam", "First."), ("Riley", "Second.")]);
    let other = Chat::new(500, &[("Alex", "Elsewhere."), ("Alex", "Again.")]);
    let mut ui = simulate(chat.view(front()));
    let second = bounds(&mut ui, "Second.");
    drag(&mut ui, end_of(second), &[Point::new(300.0, 4.0)]);
    assert!(copy(&mut ui).is_some());
    let mut ui = ui.rebuild(other.view(front()));
    assert_eq!(copy(&mut ui), None);
}

#[test]
fn content_without_rows_selects_as_one_piece() {
    use iced::widget::column;
    let first = markdown(Source::new("The **first** paragraph."));
    let second = markdown(Source::new("And the second."));
    let view = selection(column![
        paragraph::<Asked>(&first),
        text("between").size(12),
        paragraph(&second),
    ])
    .on_copy(Asked::Copied)
    .boxed();
    let mut ui = simulate(view);
    let top = bounds(&mut ui, "The first paragraph.");
    let bottom = bounds(&mut ui, "And the second.");
    drag(
        &mut ui,
        Point::new(top.x + 1.0, top.center_y()),
        &[end_of(bottom)],
    );
    // The text between selects nothing of itself: it's no part.
    assert_eq!(
        copy(&mut ui).as_deref(),
        Some("The first paragraph.\nAnd the second.")
    );
}

#[test]
fn texts_in_a_scrolled_scrollable_select_where_they_show() {
    use iced::widget::scrollable;
    let bodies: Vec<Text> = (0..30)
        .map(|at| markdown(Source::new(&format!("Paragraph {at}."))))
        .collect();
    let paragraphs = bodies.iter().map(|body| paragraph::<Asked>(body).boxed());
    let content = scrollable(Column::with_children(paragraphs).spacing(8)).height(Fill);
    let mut ui = simulate(selection(content).on_copy(Asked::Copied).boxed());
    let shows =
        |ui: &mut Ui<'_>, words: &str| ui.find(words).ok().and_then(|found| found.visible_bounds());
    assert_eq!(shows(&mut ui, "Paragraph 15."), None, "below the window");
    // The wheel scrolls it down, and the next frame shows it there.
    move_to(&mut ui, Point::new(300.0, 200.0));
    ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -300.0 },
    })]);
    frames(&mut ui, Instant::now(), Duration::from_millis(16), 1);
    let top = shows(&mut ui, "Paragraph 15.").expect("scrolled into the window");
    let bottom = shows(&mut ui, "Paragraph 16.").expect("scrolled into the window");
    drag(
        &mut ui,
        Point::new(top.x + 1.0, top.center_y()),
        &[end_of(bottom)],
    );
    assert_eq!(
        copy(&mut ui).as_deref(),
        Some("Paragraph 15.\nParagraph 16.")
    );
}
