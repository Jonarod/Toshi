use std::path::Path;

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::cluster::ClusterError;

static NODE_ID_FILENAME: &str = ".node_id";

/// Init the node id by reading the node id from path or writing a fresh one if not found
pub async fn init_node_id(path: String) -> Result<String, ClusterError> {
    let result = read_node_id(path.as_ref()).await;
    let id = match result {
        Ok(id) => Uuid::parse_str(&id).expect("Parsed node ID is not a UUID."),
        Err(_) => Uuid::new_v4(),
    };
    write_node_id(path, id.to_hyphenated().to_string()).await
}

/// Write node id to the path `p` provided, this will also append `.node_id`
pub async fn write_node_id(p: String, id: String) -> Result<String, ClusterError> {
    // Append .node_id to the path provided
    let path = Path::new(&p).join(&NODE_ID_FILENAME);
    // Create and write the id to the file and return the id
    let mut file = File::create(path).await.map_err(|e| ClusterError::FailedWritingNodeID(format!("{}", e)))?;
    file.write_all(id.as_bytes()).await.map_err(|e| ClusterError::FailedWritingNodeID(format!("{}", e)))?;
    Ok(id)
}

/// Read the node id from the file provided
///
/// Note:This function will try and Read the file as UTF-8
pub async fn read_node_id(p: &str) -> Result<String, ClusterError> {
    let path = Path::new(p).join(&NODE_ID_FILENAME);
    let mut result = String::new();
    let mut f = File::open(path).await
        .map_err(|e| ClusterError::FailedReadingNodeID(format!("{}", e)))?;
    f.read_to_string(&mut result).await
        .map_err(|e| ClusterError::FailedReadingNodeID(format!("{}", e)))?;
    Ok(result)
}

#[cfg(test)]
pub mod tests {

    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    pub fn test_node_id() -> Result<(), ClusterError> {
        let rt = Runtime::new().unwrap();
        let write_id = rt.block_on(init_node_id("./".to_string()))?;
        let read_id = rt.block_on(read_node_id("./"))?;

        assert_eq!(read_id, write_id);

        std::fs::remove_file(format!("./{}", NODE_ID_FILENAME)).unwrap();
        Ok(())
    }
}
