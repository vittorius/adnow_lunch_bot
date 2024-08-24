
// // TODO: use delegate::delegate
// // TODO: use automock and double as shown in https://docs.rs/mockall/latest/mockall/#mocking-structs
// // TODO: split this module in 2: testing::mocks & testing::factories

// #[allow(dead_code)]
// pub(crate) struct MockBot {
//     pub send_message_calls: RefCell<Vec<(ChatId, String)>>,
// }

// #[allow(dead_code)]
// impl MockBot {
//     pub fn new() -> Self {
//         Self {
//             send_message_calls: RefCell::new(vec![]),
//         }
//     }

//     pub async fn send_message<T>(&self, chat_id: ChatId, text: T) -> anyhow::Result<()>
//     where
//         T: Into<String>,
//     {
//         self.send_message_calls.borrow_mut().push((chat_id, text.into()));
//         Ok(())
//     }
// }