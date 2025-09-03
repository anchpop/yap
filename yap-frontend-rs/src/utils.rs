use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _, persistent};

pub fn set_panic_hook() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

pub(crate) async fn get_or_create_device_id(
    weapon_dir: &persistent::DirectoryHandle,
    user_id: &Option<String>,
) -> Result<String, persistent::Error> {
    let file_name = if user_id.is_some() {
        "device-id"
    } else {
        "device-id-logged-out"
    };

    let device_id_file = weapon_dir
        .get_file_handle_with_options(file_name, &opfs::GetFileHandleOptions { create: false })
        .await;

    match device_id_file {
        Ok(file_handle) => {
            // Read existing device ID
            let bytes = file_handle.read().await?;
            let device_id = String::from_utf8(bytes).unwrap_or_else(|_| {
                log::error!("Device ID file contained invalid UTF-8 data");
                eyedee::get_uuid()
            });
            Ok(device_id)
        }
        Err(_) => {
            // Generate new device ID
            let device_id = eyedee::get_uuid();

            // Save it to OPFS
            let mut file_handle = weapon_dir
                .get_file_handle_with_options(
                    file_name,
                    &opfs::GetFileHandleOptions { create: true },
                )
                .await?;

            let mut writable = file_handle
                .create_writable_with_options(&opfs::CreateWritableOptions {
                    keep_existing_data: false,
                })
                .await?;

            writable
                .write_at_cursor_pos(device_id.as_bytes().to_vec())
                .await?;

            writable.close().await?;

            Ok(device_id)
        }
    }
}

pub async fn hit_ai_server(
    path: &str,
    request: impl serde::Serialize,
    access_token: Option<&String>,
) -> Result<fetch_happen::Response, fetch_happen::Error> {
    let client = fetch_happen::Client;
    let url = if cfg!(feature = "local-backend") {
        "http://localhost:8080"
    } else {
        "https://yap-ai-backend.fly.dev"
    };
    let mut request_builder = client.post(format!("{url}{path}")).json(&request)?;
    if let Some(token) = access_token {
        request_builder = request_builder.header("Authorization", format!("Bearer {token}"));
    }
    let response = request_builder.send().await?;
    Ok(response)
}
