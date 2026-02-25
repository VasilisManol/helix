use helix_event::register_hook;
use helix_view::events::DocumentDidClose;
use helix_view::handlers::Handlers;

pub(super) fn register_hooks(_handlers: &Handlers) {
    register_hook!(move |event: &mut DocumentDidClose<'_>| {
        if let Some(peer_id) = event.doc.side_diff_peer_id() {
            if let Some(peer_doc) = event.editor.documents.get_mut(&peer_id) {
                peer_doc.clear_side_diff_peer();
            }
        }
        Ok(())
    });
}
