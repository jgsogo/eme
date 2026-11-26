use tokio::sync::oneshot;

pub(crate) enum PendingReply {
    Text { sender: oneshot::Sender<String> },
    YesNo { sender: oneshot::Sender<bool> },
    Choice { sender: oneshot::Sender<String> },
}
