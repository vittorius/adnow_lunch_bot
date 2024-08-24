use chrono::Utc;
use teloxide::types::{
    Chat, ChatId, ChatKind, ChatPermissions, ChatPublic, MediaKind, MediaText, Message, MessageCommon, MessageId,
    MessageKind, PublicChatGroup, PublicChatKind, User, UserId,
};

beaver::define! {
    pub MessageFactory (Message) {
        id -> |n| MessageId(n as i32),
        thread_id -> |n| Some(n as i32),
        date -> |_| Utc::now(),
        chat -> ChatFactory::build,
        via_bot -> |_| None,
        kind -> |n| MessageKind::Common(MessageCommonFactory::build(n))
    }
}

beaver::define! {
    MessageCommonFactory (MessageCommon) {
        from -> |n| Some(UserFactory::build(n)),
        sender_chat -> |n| Some(ChatFactory::build(n)),
        author_signature -> |n| Some(format!("Author {}", n)),
        forward -> |_| None,
        reply_to_message -> |_| None,
        edit_date -> |_| None,
        media_kind -> |_| MediaKind::Text(MediaText { text: "Default text".to_string(), entities: vec![] }),
        reply_markup -> |_| None,
        is_topic_message -> |_| false,
        is_automatic_forward -> |_| false,
        has_protected_content -> |_| false
    }
}

beaver::define! {
    UserFactory (User) {
        id -> |n| UserId(n as u64),
        is_bot -> |_| false,
        first_name -> |n| format!("John {}", n),
        last_name -> |_| Some("Doe".to_string()),
        username -> |n| Some(format!("user_{}", n)),
        language_code -> |_| Some("en".to_string()),
        is_premium -> |_| false,
        added_to_attachment_menu -> |_| false
    }
}

beaver::define! {
    ChatFactory (Chat) {
        id -> |n| ChatId(n as i64),
        kind -> |n| ChatKind::Public(ChatPublicFactory::build(n)),
        has_aggressive_anti_spam_enabled -> |_| false,
        has_hidden_members -> |_| false,
        message_auto_delete_time -> |_| None,
        photo -> |_| None,
        pinned_message -> |_| None
    }
}

beaver::define! {
    ChatPublicFactory (ChatPublic) {
        title -> |n| Some(format!("Group Chat {n}")),
        description -> |_| None,
        kind -> |n| PublicChatKind::Group(PublicChatGroupFactory::build(n)),
        has_protected_content -> |_| None,
        invite_link -> |_| None
    }
}

beaver::define! {
    PublicChatGroupFactory (PublicChatGroup) {
        permissions -> |_| Some(ChatPermissions::all())
    }
}
