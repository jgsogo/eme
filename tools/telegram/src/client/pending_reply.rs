use tokio::sync::oneshot;

pub(super) enum PendingReply {
    Text { sender: oneshot::Sender<String> },
    YesNo { sender: oneshot::Sender<bool> },
}
