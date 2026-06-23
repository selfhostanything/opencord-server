use opencord_server::config::AppConfig;
use opencord_server::local_seed::{LocalAlphaSeedOptions, seed_local_alpha};
use opencord_server::state::AppState;

fn test_state() -> AppState {
    AppState::in_memory(AppConfig {
        version: "test-version".to_owned(),
        public_url: "https://chat.example.com".to_owned(),
    })
}

#[tokio::test]
async fn local_alpha_seed_creates_repeatable_demo_workspace() {
    let state = test_state();

    let first = seed_local_alpha(&state, LocalAlphaSeedOptions::default())
        .await
        .expect("first seed should succeed");
    let second = seed_local_alpha(&state, LocalAlphaSeedOptions::default())
        .await
        .expect("second seed should succeed");

    assert_eq!(first.owner.user_id, second.owner.user_id);
    assert_eq!(first.organization.id, second.organization.id);
    assert_eq!(first.space.id, second.space.id);
    assert_eq!(first.channels.text.id, second.channels.text.id);
    assert_eq!(first.channels.voice.id, second.channels.voice.id);
    assert_eq!(first.messages.welcome.id, second.messages.welcome.id);
    assert_eq!(first.messages.rich.id, second.messages.rich.id);
    assert_eq!(
        first.messages.attachment_fixture.id,
        second.messages.attachment_fixture.id
    );
    assert_eq!(first.meeting.id, second.meeting.id);
    assert_eq!(first.bot.application_id, second.bot.application_id);
    assert_eq!(first.webhook.id, second.webhook.id);

    assert_ne!(first.owner.session_token, second.owner.session_token);
    assert_ne!(first.bot.token, second.bot.token);
    assert_ne!(first.webhook.token, second.webhook.token);

    let organizations = state
        .organizations
        .list_for_user(second.owner.user_id)
        .await
        .expect("list organizations");
    assert_eq!(organizations.len(), 1);

    let spaces = state
        .spaces
        .list_for_user(second.owner.user_id, second.organization.id)
        .await
        .expect("list spaces");
    assert_eq!(spaces.len(), 1);

    let channels = state
        .channels
        .list_for_space(second.space.id)
        .await
        .expect("list channels");
    assert_eq!(channels.len(), 2);

    let messages = state
        .messages
        .list_for_channel(second.channels.text.id)
        .await
        .expect("list messages");
    assert_eq!(messages.len(), 3);
    assert!(messages.iter().any(|message| message.embeds.len() == 1));
    assert!(messages.iter().any(|message| message.components.len() == 1));
    assert!(messages.iter().any(|message| message.mention_everyone));
    assert!(
        messages
            .iter()
            .any(|message| message.reply_to_message_id == Some(second.messages.welcome.id))
    );

    let attachments = state
        .attachments
        .list_for_message_ids(&[second.messages.attachment_fixture.id])
        .await
        .expect("list attachment fixture");
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].file_name, "local-alpha-readme.txt");
    assert_eq!(
        attachments[0].message_id,
        Some(second.messages.attachment_fixture.id)
    );

    let bot = state
        .bots
        .authenticate_token(&second.bot.token)
        .await
        .expect("seeded bot token should authenticate");
    assert_eq!(bot.application_id, second.bot.application_id);
    assert_eq!(bot.organization_id, second.organization.id);

    let webhook = state
        .webhooks
        .verify(second.webhook.id, &second.webhook.token)
        .await
        .expect("seeded webhook token should verify");
    assert_eq!(webhook.channel_id, second.channels.text.id);
}

#[test]
fn makefile_exposes_local_seed_target() {
    let makefile = std::fs::read_to_string("Makefile").expect("read Makefile");

    assert!(
        makefile.contains("seed:"),
        "Makefile should expose a deterministic local alpha seed target"
    );
    assert!(
        makefile.contains("cargo run --bin seed"),
        "seed target should run the Rust seed binary"
    );
}
