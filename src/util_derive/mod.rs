use anyhow::Result;
use syn::File;


pub async fn parse_file(content: impl AsRef<str>) -> Result<File> {
    Ok(syn::parse_file(content.as_ref())?)
}