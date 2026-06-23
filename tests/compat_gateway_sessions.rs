use std::sync::Arc;

use opencord_server::domain::bot::AuthenticatedBot;
use opencord_server::domain::compat_gateway::{
    CompatGatewayReplayEvent, CompatGatewayResumeResult, CompatGatewaySessions,
};
use opencord_server::repositories::compat_gateway_memory::MemoryCompatGatewaySessionStore;
use serde_json::json;
use uuid::Uuid;

fn test_bot() -> AuthenticatedBot {
    AuthenticatedBot {
        application_id: Uuid::now_v7(),
        organization_id: Uuid::now_v7(),
        bot_user_id: Uuid::now_v7(),
        name: "Gateway Bot".to_owned(),
    }
}

#[tokio::test]
async fn compat_gateway_session_store_resumes_and_preserves_max_sequence() {
    let sessions = CompatGatewaySessions::new(Arc::new(MemoryCompatGatewaySessionStore::default()));
    let bot = test_bot();

    sessions
        .create("gw_session".to_owned(), &bot, 1, 512)
        .await
        .expect("create session");
    sessions
        .update_sequence("gw_session", 7)
        .await
        .expect("update sequence");
    sessions
        .update_sequence("gw_session", 3)
        .await
        .expect("ignore lower sequence");

    let result = sessions
        .resume("gw_session", &bot, 4)
        .await
        .expect("resume session");

    let CompatGatewayResumeResult::Resumed(session) = result else {
        panic!("expected resumed session");
    };
    assert_eq!(session.session_id, "gw_session");
    assert_eq!(session.application_id, bot.application_id);
    assert_eq!(session.organization_id, bot.organization_id);
    assert_eq!(session.bot_user_id, bot.bot_user_id);
    assert_eq!(session.sequence, 7);
    assert_eq!(session.intents, 512);
}

#[tokio::test]
async fn compat_gateway_session_store_rejects_wrong_bot_and_future_sequence() {
    let sessions = CompatGatewaySessions::new(Arc::new(MemoryCompatGatewaySessionStore::default()));
    let bot = test_bot();
    let mut other_bot = test_bot();
    other_bot.application_id = Uuid::now_v7();

    sessions
        .create("gw_session".to_owned(), &bot, 2, 513)
        .await
        .expect("create session");

    let wrong_bot_result = sessions
        .resume("gw_session", &other_bot, 1)
        .await
        .expect("resume wrong bot");
    assert_eq!(wrong_bot_result, CompatGatewayResumeResult::NotFound);

    let future_sequence_result = sessions
        .resume("gw_session", &bot, 99)
        .await
        .expect("resume future sequence");
    assert_eq!(
        future_sequence_result,
        CompatGatewayResumeResult::InvalidSequence
    );
}

#[tokio::test]
async fn compat_gateway_session_store_lists_replay_events_after_client_sequence() {
    let sessions = CompatGatewaySessions::new(Arc::new(MemoryCompatGatewaySessionStore::default()));
    let bot = test_bot();

    sessions
        .create("gw_session".to_owned(), &bot, 1, 512)
        .await
        .expect("create session");
    for (sequence, event_type, payload) in [
        (4, "MESSAGE_UPDATE", json!({ "id": "message-4" })),
        (2, "MESSAGE_CREATE", json!({ "id": "message-2" })),
        (3, "MESSAGE_DELETE", json!({ "id": "message-3" })),
    ] {
        sessions
            .append_replay_event(CompatGatewayReplayEvent {
                session_id: "gw_session".to_owned(),
                sequence,
                event_type: event_type.to_owned(),
                payload,
            })
            .await
            .expect("append replay event");
    }

    let replayed = sessions
        .list_replay_events_after("gw_session", 2, 10)
        .await
        .expect("list replay events");

    assert_eq!(
        replayed
            .iter()
            .map(|event| (event.sequence, event.event_type.as_str()))
            .collect::<Vec<_>>(),
        vec![(3, "MESSAGE_DELETE"), (4, "MESSAGE_UPDATE")]
    );
    assert_eq!(replayed[0].payload["id"], "message-3");
    assert_eq!(replayed[1].payload["id"], "message-4");

    let limited = sessions
        .list_replay_events_after("gw_session", 1, 2)
        .await
        .expect("list limited replay events");
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].sequence, 2);
    assert_eq!(limited[1].sequence, 3);
}
