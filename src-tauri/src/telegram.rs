use serde::Serialize;
use std::time::Duration;

#[derive(Serialize)]
struct SendMessagePayload<'a> {
    chat_id: &'a str,
    text: &'a str,
}

/// Asynchronously dispatches a plain text message to a Telegram chat via Bot API.
pub async fn send_telegram_message(bot_token: &str, chat_id: &str, text: &str) -> Result<(), String> {
    let token = bot_token.trim();
    let chat = chat_id.trim();

    if token.is_empty() || chat.is_empty() {
        return Err("Bot Token y Chat ID no pueden estar vacíos".to_string());
    }

    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let payload = SendMessagePayload {
        chat_id: chat,
        text: text.trim(),
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Error creando cliente HTTP: {e}"))?;

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Error de conexión con Telegram: {e}"))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(format!("Telegram API devolvió HTTP {status}: {body}"))
    }
}

/// Tests the credentials by sending a confirmation test message.
pub async fn test_telegram_connection(bot_token: &str, chat_id: &str) -> Result<String, String> {
    let test_msg = "🤖 ¡Conexión exitosa! Las notificaciones de Pomotroid están listas.";
    send_telegram_message(bot_token, chat_id, test_msg).await?;
    Ok("Mensaje de prueba enviado exitosamente a tu Telegram.".to_string())
}
