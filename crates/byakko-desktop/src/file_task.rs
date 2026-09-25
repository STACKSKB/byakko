//! Keep blocking local file work off Iced's async executor.
use iced::Task;

pub(super) fn spawn_blocking<Value, Message>(
    thread_name: &'static str,
    stopped_message: &'static str,
    work: impl FnOnce() -> Result<Value, String> + Send + 'static,
    on_complete: impl FnOnce(Result<Value, String>) -> Message + Send + 'static,
) -> Task<Message>
where
    Value: Send + 'static,
    Message: Send + 'static,
{
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    if let Err(error) = std::thread::Builder::new()
        .name(thread_name.into())
        .spawn(move || {
            let _ = sender.send(work());
        })
    {
        return Task::done(on_complete(Err(error.to_string())));
    }
    Task::perform(
        async move {
            receiver
                .await
                .unwrap_or_else(|_| Err(stopped_message.into()))
        },
        on_complete,
    )
}
