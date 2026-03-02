use super::*;

use std::time::Duration;

use helix_core::{Position, Transaction};
use helix_stdx::path;
use helix_term::{application::Application, args::Args};
use helix_view::{tree::Layout, DocumentId, Editor};
use helix_view::input::parse_macro;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[cfg(windows)]
use crossterm::event::{Event, KeyEvent};
#[cfg(not(windows))]
use termina::event::{Event, KeyEvent};

fn doc_text(editor: &Editor, doc_id: DocumentId) -> String {
    editor
        .documents
        .get(&doc_id)
        .expect("document exists")
        .text()
        .slice(..)
        .to_string()
}

async fn wait_for_diff_base(
    editor: &Editor,
    doc_id: DocumentId,
    expected: &str,
) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    loop {
        if let Some(doc) = editor.documents.get(&doc_id) {
            if let Some(diff_handle) = doc.diff_handle() {
                let diff = diff_handle.load();
                if diff.diff_base().slice(..).to_string() == expected {
                    return Ok(());
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("diff base did not update within timeout");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn side_diff_updates_on_both_sides() -> anyhow::Result<()> {
    let file_left = helpers::temp_file_with_contents("left\n")?;
    let file_right = helpers::temp_file_with_contents("right\n")?;

    let left_path = path::canonicalize(file_left.path());
    let right_path = path::canonicalize(file_right.path());

    let mut args = Args::default();
    args.diff_mode = true;
    args.split = Some(Layout::Vertical);
    args.files
        .insert(left_path.clone(), vec![Position::default()]);
    args.files
        .insert(right_path.clone(), vec![Position::default()]);

    let mut app = Application::new(args, helpers::test_config(), helpers::test_syntax_loader(None))?;

    let left_id = app
        .editor
        .document_id_by_path(&left_path)
        .expect("left document open");
    let right_id = app
        .editor
        .document_id_by_path(&right_path)
        .expect("right document open");

    let right_text = doc_text(&app.editor, right_id);
    wait_for_diff_base(&app.editor, left_id, &right_text).await?;

    let left_text = doc_text(&app.editor, left_id);
    wait_for_diff_base(&app.editor, right_id, &left_text).await?;

    {
        let doc = app.editor.documents.get_mut(&right_id).unwrap();
        let view_id = *doc.selections().keys().next().unwrap();
        let transaction =
            Transaction::insert(doc.text(), doc.selection(view_id), "right edit\n".into());
        doc.apply(&transaction, view_id);
    }

    let right_text = doc_text(&app.editor, right_id);
    wait_for_diff_base(&app.editor, left_id, &right_text).await?;

    {
        let doc = app.editor.documents.get_mut(&left_id).unwrap();
        let view_id = *doc.selections().keys().next().unwrap();
        let transaction =
            Transaction::insert(doc.text(), doc.selection(view_id), "left edit\n".into());
        doc.apply(&transaction, view_id);
    }

    let left_text = doc_text(&app.editor, left_id);
    wait_for_diff_base(&app.editor, right_id, &left_text).await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn diff_command_opens_files_and_sets_diff() -> anyhow::Result<()> {
    let file_left = helpers::temp_file_with_contents("left\n")?;
    let file_right = helpers::temp_file_with_contents("right\n")?;

    let left_path = path::canonicalize(file_left.path());
    let right_path = path::canonicalize(file_right.path());

    let mut app = helpers::AppBuilder::new().build()?;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut rx_stream = UnboundedReceiverStream::new(rx);

    let command = format!(
        ":diff {} {}<ret>",
        left_path.display(),
        right_path.display()
    );
    for key_event in parse_macro(&command)?.into_iter() {
        tx.send(Ok(Event::Key(KeyEvent::from(key_event))))?;
    }

    let app_exited = !app.event_loop_until_idle(&mut rx_stream).await;
    assert!(!app_exited);

    assert_eq!(app.editor.tree.views().count(), 2);

    let left_id = app
        .editor
        .document_id_by_path(&left_path)
        .expect("left document open");
    let right_id = app
        .editor
        .document_id_by_path(&right_path)
        .expect("right document open");

    let right_text = doc_text(&app.editor, right_id);
    wait_for_diff_base(&app.editor, left_id, &right_text).await?;

    let left_text = doc_text(&app.editor, left_id);
    wait_for_diff_base(&app.editor, right_id, &left_text).await?;

    let left_doc = app.editor.documents.get(&left_id).unwrap();
    assert_eq!(left_doc.side_diff_peer_id(), Some(right_id));
    let right_doc = app.editor.documents.get(&right_id).unwrap();
    assert_eq!(right_doc.side_diff_peer_id(), Some(left_id));

    let errs = app.close().await;
    if !errs.is_empty() {
        for err in errs {
            eprintln!("Error closing app: {err}");
        }
        anyhow::bail!("Error closing app");
    }

    Ok(())
}
