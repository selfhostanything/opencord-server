use std::sync::Arc;

use opencord_server::domain::bot::AuthenticatedBot;
use opencord_server::domain::compat_gateway::{CompatGatewayResumeResult, CompatGatewaySessions};
use opencord_server::repositories::compat_gateway_memory::MemoryCompatGatewaySessionStore;
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
