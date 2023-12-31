use allowance::{Merchant, Allowance, User};
use lambda_http::{run, service_fn, Body, Error, Request, Response};
use serde_json::json;
use firestore::FirestoreDb;
use chrono::Utc;
use uuid::Uuid;

#[derive(serde::Deserialize, serde::Serialize, Default)]
struct ApiEntryReq
{
	email: String,
	username: String,
	password: String,
	want_secure_token: Option<bool>,
}

async fn function_handler(event: Request) -> Result<Response<Body>, Error>
{
	// Extract some useful information from the request

	let entry_req: ApiEntryReq = serde_json::from_slice(
		event
		.body())
		.unwrap_or(ApiEntryReq::default());

	if entry_req.username.contains(" ") || entry_req.username.contains("@")
	{
		eprintln!("Invalid Username Character");
		let json_body = json!(
			{
				"failed": true,
				"message": "invalid characters in username",
			}
			).to_string();
		return Ok(Response::builder()
			.status(500)
			.header("content-type", "application/json")
			.body(json_body.into())
			.map_err(Box::new)?);
	}

	match std::env::var("FIREBASE_WEB_API_KEY")
	{
		Ok(api_key) => 
		{
			// check if username already exists
			let db = FirestoreDb::new("allowance-fa781").await?;

			let existing_users = db.fluent()
									.select()
									.from("users")
									.filter(|d|
									{
										d.field("username").eq(&entry_req.username)
									})
									.obj::<User>()
									.query()
									.await?;

			if !existing_users.is_empty()
			{
				eprintln!("Username Already Exists");
				let json_body = json!(
					{
						"failed": true,
						"message": "username already exists",
					}
					).to_string();
				return Ok(Response::builder()
					.status(500)
					.header("content-type", "application/json")
					.body(json_body.into())
					.map_err(Box::new)?);
			}

			// sign up with firebase auth
			let auth = firebase_auth_sdk::FireAuth::new(api_key);
			let result = auth.sign_up_email(&entry_req.email, &entry_req.password, entry_req.want_secure_token.unwrap_or(false)).await;

			match result
			{
				Ok(res) =>
				{
					let user = User {
						email: res.email,
						username: entry_req.username.to_lowercase(),
						created_at: Utc::now().to_rfc3339(),
						photo_url: None,
						user_id: res.local_id,
						email_verification_token: Uuid::new_v4().to_string(),
						email_verified: false,
						test: None,
						is_public: Some(false),
					};
					
					// insert user into db
					db.fluent()
						.insert()
						.into("users")
						.document_id(&user.user_id)
						.object::<User>(&user)
						.execute::<User>()
						.await?;

					// get merchants available to new user
					let merchant = db.fluent()
										.select()
										.by_id_in("merchants")
										.obj::<Merchant>()
										.one("zzy3wQDdmwXXjzVu4eCx3QRAQ1J3")
										.await?.unwrap();

					let allowance = Allowance {
						amount: "$0.00".to_string(),
						merchant_uid: merchant.merchant_uid,
					};

					let parent_path = db.parent_path("users", &user.user_id)?;

					db.fluent()
						.insert()
						.into("allowance")
						.document_id(&allowance.merchant_uid)
						.parent(&parent_path)
						.object::<Allowance>(&allowance)
						.execute()
						.await?;

					// Send a confirmation email
					let sendgrid_env_var = std::env::var("SENDGRID_API_KEY");
					match sendgrid_env_var
					{
						Ok(token) => 
						{
							let client = reqwest::Client::new();
							client.post("https://api.sendgrid.com/v3/mail/send")
								.bearer_auth(token)
								.header("content-type", "application/json")
								.body(json!(
								{
									"from":{
										"email":"confirmation@allowance.fund",
										"name":"Hoya Allowance",
									 },
									"personalizations":
									[
										{
											"to":[
													{
													   "email": entry_req.email,
													},
												 ],
											"dynamic_template_data":
											{
												"username": entry_req.username,
												"uid": user.user_id,
												"token": user.email_verification_token,
											}
										}
									],
									"template_id":"d-f9bdc147ca1847b59ff50ea3be406da5"
								}
								).to_string())
								.send()
								.await?;

							Ok(Response::builder()
								.status(200)
								.header("content-type", "application/json")
								.body(
									if entry_req.want_secure_token.unwrap_or(false)
									{
										json!(
											{
											}
										).to_string().into()
									}
									else 
									{
										json!({}).to_string().into() 
									})
								.map_err(Box::new)?)
						},
						Err(_) =>
						{
							eprintln!("SENDGRID_API_KEY is not set");
							let json_body = json!({}).to_string();
							Ok(Response::builder()
								.status(200)
								.header("content-type", "application/json")
								.body(json_body.into())
								.map_err(Box::new)?)
						}
					}
				},
				Err(_) =>
				{
					let json_body = json!({"failed": true}).to_string();
					Ok(Response::builder()
						.status(5001)
						.header("content-type", "application/json")
						.body(json_body.into())
						.map_err(Box::new)?)
				},
			}
		},
		Err(_) => 
		{
			eprintln!("FIREBASE_WEB_API_KEY is not set");
			let json_body = json!(
				{
					"failed": true,
					"message": "FIREBASE_WEB_API_KEY is not set",
				}
				).to_string();
			Ok(Response::builder()
				.status(500)
				.header("content-type", "application/json")
				.body(json_body.into())
				.map_err(Box::new)?)
		},
	}
}

#[tokio::main]
async fn main() -> Result<(), Error>
{
	tracing_subscriber::fmt()
		.with_max_level(tracing::Level::INFO)
		// disable printing the name of the module in every log line.
		.with_target(false)
		// disabling time is handy because CloudWatch will add the ingestion time.
		.without_time()
		.init();

	run(service_fn(function_handler)).await
}
