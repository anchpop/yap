use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _};

#[bridgerton::bridge]
pub async fn test_opfs() -> Result<bool, bridgerton::Error> {
    log::info!("Testing OPFS support...");

    let mut root = match opfs::persistent::app_specific_dir().await {
        Ok(root) => root,
        Err(e) => {
            log::error!("Failed to get OPFS root: {e:?}");
            return Ok(false);
        }
    };

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

    let test_data = b"OPFS test data";
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

    if let Err(e) = writable.write_at_cursor_pos(test_data).await {
        log::error!("Failed to write data: {e:?}");
        return Ok(false);
    }

    if let Err(e) = writable.close().await {
        log::error!("Failed to close writable: {e:?}");
        return Ok(false);
    }

    let read_data = match test_file.read().await {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to read data: {e:?}");
            return Ok(false);
        }
    };

    if read_data != test_data {
        log::error!("OPFS test failed: Data mismatch.");
        return Ok(false);
    }

    if let Err(e) = test_dir.remove_entry("test.txt").await {
        log::error!("Failed to delete test file: {e:?}");
    }

    if let Err(e) = root.remove_entry("opfs-test").await {
        log::error!("Failed to delete test directory: {e:?}");
    }

    log::info!("OPFS test passed!");
    Ok(true)
}
