use super::*;

use std::time::Duration;

use helix_core::{Position, Transaction};
use helix_stdx::path;
use helix_term::{application::Application, args::Args};
use helix_view::{tree::Layout, DocumentId, Editor};

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
