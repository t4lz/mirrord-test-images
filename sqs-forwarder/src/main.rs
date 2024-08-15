use aws_sdk_sqs::{operation::receive_message::ReceiveMessageOutput, types::Message, Client};
use tokio::time::{sleep, Duration};

/// Name of the environment variable that holds the name of the SQS queue to read from.
const QUEUE_NAME_ENV_VAR: &str = "SQS_TEST_Q_NAME";

/// Name of the environment variable that holds the name of the SQS fifo queue to read from.
const FIFO_QUEUE_NAME_ENV_VAR: &str = "SQS_TEST_FIFO_Q_NAME";

async fn read_from_queue(read_q_name: String, client: Client) -> Result<(), anyhow::Error> {
    println!("Q Name: {read_q_name}");
    let read_q_url = client
        .get_queue_url()
        .queue_name(read_q_name)
        .send()
        .await?
        .queue_url
        .unwrap();
    let receive_message_request = client
        .receive_message()
        .message_attribute_names(".*")
        // Without this the wait time would be 0 and responses would return immediately also when
        // there are no messages (and potentially even sometimes return empty immediately when
        // there are actually messages).
        // By setting a time != 0 (20 is the maximum), we perform "long polling" which means we
        // won't get "false empties" and also less empty responses, because SQS will wait for that
        // time before returning an empty response.
        .wait_time_seconds(20)
        .queue_url(&read_q_url);
    loop {
        let res = match receive_message_request.clone().send().await {
            Ok(res) => res,
            Err(err) => {
                println!("ERROR: {err:?}");
                sleep(Duration::from_secs(3)).await;
                continue;
            }
        };
        if let ReceiveMessageOutput {
            messages: Some(messages),
            ..
        } = res
        {
            for Message {
                body,
                message_attributes,
                message_id,
                receipt_handle,
                ..
            } in messages
            {
                println!("message_id: {message_id:?}, message_attributes: {message_attributes:?}, body: {body:?}");
                if let Some(handle) = receipt_handle {
                    client
                        .delete_message()
                        .queue_url(&read_q_url)
                        .receipt_handle(handle)
                        .send()
                        .await?;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let sdk_config = aws_config::load_from_env().await;
    let client = Client::new(&sdk_config);
    let read_q_name = std::env::var(QUEUE_NAME_ENV_VAR).unwrap();
    let q_task_handle = tokio::spawn(read_from_queue(read_q_name.clone(), client.clone()));
    let read_q_name = std::env::var(FIFO_QUEUE_NAME_ENV_VAR).unwrap();
    let fifo_q_task_handle = tokio::spawn(read_from_queue(read_q_name.clone(), client.clone()));
    let (q_res, fifo_res) = tokio::join!(q_task_handle, fifo_q_task_handle);
    q_res.unwrap().unwrap();
    fifo_res.unwrap().unwrap();
}