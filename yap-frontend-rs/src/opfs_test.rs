use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _};
use wasm_bindgen::prelude::*;

// OPFS test to verify browser support
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn test_opfs() -> Result<bool, JsValue> {
    // Call the inner function
    let result = test_opfs_inner().await;

    // Print the result
    match &result {
        Ok(success) => {
            if *success {
                log::info!("OPFS test result: Success - OPFS is supported");
            } else {
                log::info!("OPFS test result: Failure - OPFS is not supported");
            }
        }
        Err(_) => {
            log::error!("OPFS test result: An error occurred during testing");
        }
    }

    // Return the result
    result
}

// Inner function that performs the actual OPFS test
async fn test_opfs_inner() -> Result<bool, JsValue> {
    log::info!("Testing OPFS support...");

    // Get the root directory
    let mut root = match opfs::persistent::app_specific_dir().await {
        Ok(root) => root,
        Err(e) => {
            log::error!("Failed to get OPFS root: {e:?}");
            return Ok(false);
        }
    };

    // Create a test directory
    let mut test_dir = match root
        .get_directory_handle_with_options(
            "opfs-test",
            &opfs::GetDirectoryHandleOptions { create: true },
        )
        .await
    {
        Ok(dir) => dir,
        Err(e) => {
            log::error!("Failed to create test directory: {e:?}");
            return Ok(false);
        }
    };

    // Create a test file
    let mut test_file = match test_dir
        .get_file_handle_with_options("test.txt", &opfs::GetFileHandleOptions { create: true })
        .await
    {
        Ok(file) => file,
        Err(e) => {
            log::error!("Failed to create test file: {e:?}");
            return Ok(false);
        }
    };

    // Write test data
    let test_data = b"OPFS test data".to_vec();
    let mut writable = match test_file
        .create_writable_with_options(&opfs::CreateWritableOptions {
            keep_existing_data: false,
        })
        .await
    {
        Ok(writable) => writable,
        Err(e) => {
            log::error!("Failed to create writable: {e:?}");
            return Ok(false);
        }
    };

    if let Err(e) = writable.write_at_cursor_pos(test_data.clone()).await {
        log::error!("Failed to write data: {e:?}");
        return Ok(false);
    }

    if let Err(e) = writable.close().await {
        log::error!("Failed to close writable: {e:?}");
        return Ok(false);
    }

    // Read the data back
    let read_data = match test_file.read().await {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to read data: {e:?}");
            return Ok(false);
        }
    };

    // Verify the data matches
    if read_data != test_data {
        log::error!("OPFS test failed: Data mismatch.");
        return Ok(false);
    }

    // Delete the test file
    if let Err(e) = test_dir.remove_entry("test.txt").await {
        log::error!("Failed to delete test file: {e:?}");
        // Not critical, continue
    }

    // Delete the test directory
    if let Err(e) = root.remove_entry("opfs-test").await {
        log::error!("Failed to delete test directory: {e:?}");
        // Not critical, continue
    }

    log::info!("OPFS test passed!");
    Ok(true)
}
