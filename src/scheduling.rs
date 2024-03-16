use tokio::time;

pub async fn schedule_loop() {
    let mut check_interval = time::interval(time::Duration::from_secs(10));
    loop {
        check_interval.tick().await;
    }
}
