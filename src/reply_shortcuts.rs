use std::sync::Arc;

use serenity::{
	all::CommandInteraction,
	async_trait,
	builder::{
		CreateAttachment, CreateInteractionResponse, CreateInteractionResponseFollowup,
		CreateInteractionResponseMessage,
	},
	http::Http,
	Result as SerenityResult,
};

#[async_trait]
pub trait ReplyShortcuts {
	async fn reply<S>(&self, http: &Arc<Http>, content: S, ephemeral: bool) -> SerenityResult<()>
	where
		S: Into<String> + Send;
	async fn ephemeral_reply<S>(&self, http: &Arc<Http>, content: S) -> SerenityResult<()>
	where
		S: Into<String> + std::marker::Send;
	async fn public_reply<S>(&self, http: &Arc<Http>, content: S) -> SerenityResult<()>
	where
		S: Into<String> + std::marker::Send;
	async fn reply_image(
		&self,
		http: &Arc<Http>,
		image: &[u8],
		file_name: &str,
		ephemeral: bool,
	) -> SerenityResult<()>;
	async fn public_reply_image(
		&self,
		http: &Arc<Http>,
		image: &[u8],
		file_name: &str,
	) -> SerenityResult<()>;
	async fn ephemeral_reply_image(
		&self,
		http: &Arc<Http>,
		image: &[u8],
		file_name: &str,
	) -> SerenityResult<()>;
	async fn follow_up_image(
		&self,
		http: &Arc<Http>,
		image: &[u8],
		file_name: &str,
	) -> SerenityResult<serenity::all::Message>;
}

#[async_trait]
impl ReplyShortcuts for CommandInteraction {
	async fn reply<S>(&self, http: &Arc<Http>, content: S, ephemeral: bool) -> SerenityResult<()>
	where
		S: Into<String> + Send,
	{
		self.create_response(
			http,
			CreateInteractionResponse::Message(
				CreateInteractionResponseMessage::new()
					.content(content)
					.ephemeral(ephemeral),
			),
		)
		.await
	}
	async fn ephemeral_reply<S>(&self, http: &Arc<Http>, content: S) -> SerenityResult<()>
	where
		S: Into<String> + Send,
	{
		self.reply(http, content, true).await
	}
	async fn public_reply<S>(&self, http: &Arc<Http>, content: S) -> SerenityResult<()>
	where
		S: Into<String> + Send,
	{
		self.reply(http, content, false).await
	}
	async fn reply_image(
		&self,
		http: &Arc<Http>,
		image: &[u8],
		file_name: &str,
		ephemeral: bool,
	) -> SerenityResult<()> {
		self.create_response(
			http,
			CreateInteractionResponse::Message(
				CreateInteractionResponseMessage::new()
					.add_file(CreateAttachment::bytes(image, file_name))
					.ephemeral(ephemeral),
			),
		)
		.await
	}
	async fn public_reply_image(
		&self,
		http: &Arc<Http>,
		image: &[u8],
		file_name: &str,
	) -> SerenityResult<()> {
		self.reply_image(http, image, file_name, false).await
	}
	async fn ephemeral_reply_image(
		&self,
		http: &Arc<Http>,
		image: &[u8],
		file_name: &str,
	) -> SerenityResult<()> {
		self.reply_image(http, image, file_name, true).await
	}
	async fn follow_up_image(
		&self,
		http: &Arc<Http>,
		image: &[u8],
		file_name: &str,
	) -> SerenityResult<serenity::all::Message> {
		self.create_followup(
			http,
			CreateInteractionResponseFollowup::new()
				.add_file(CreateAttachment::bytes(image, file_name)),
		)
		.await
	}
}
