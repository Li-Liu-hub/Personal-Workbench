use std::{fs,io::Write,path::{Component,Path,PathBuf}};
use crate::error::{DeskError,Result};
fn assets_directory(data:&Path)->Result<PathBuf>{
 let root=data.canonicalize()?;let dir=data.join("writing-assets");fs::create_dir_all(&dir)?;let canonical=dir.canonicalize()?;
 if !canonical.starts_with(&root) {return Err(DeskError::InvalidPath("图片目录不在数据目录内".into()))} Ok(canonical)
}
fn format(bytes:&[u8])->Result<(&'static str,&'static str)>{
 match image::guess_format(bytes).map_err(|_|DeskError::UnsupportedFormat)?{image::ImageFormat::Png=>Ok(("png","image/png")),image::ImageFormat::Jpeg=>Ok(("jpg","image/jpeg")),image::ImageFormat::WebP=>Ok(("webp","image/webp")),image::ImageFormat::Bmp=>Ok(("bmp","image/bmp")),_=>Err(DeskError::UnsupportedFormat)}
}
/// Store an independent copy in the existing writing-assets export directory.
pub fn import(data:&Path,source:&Path)->Result<String>{
 let bytes=crate::contacts::import_photo(source)?;import_bytes(data,&bytes)
}
/// Save validated clipboard image bytes as a managed writing asset.
pub fn import_bytes(data:&Path,bytes:&[u8])->Result<String>{
 if bytes.len()>crate::contacts::MAX_FILE_BYTES{return Err(DeskError::InvalidPath("图片不能超过 8 MiB".into()))}
 let (ext,_)=format(bytes)?;crate::theme::decode(bytes)?;let root=assets_directory(data)?;
 let name=format!("{}.{}",uuid::Uuid::new_v4(),ext);let mut file=fs::OpenOptions::new().write(true).create_new(true).open(root.join(&name))?;file.write_all(bytes)?;file.sync_all()?;Ok(format!("writing-assets/{name}"))
}
fn decode_percent_encoding(value:&str)->Result<String>{
 let bytes=value.as_bytes();let mut decoded=Vec::with_capacity(bytes.len());let mut index=0;
 while index<bytes.len(){
  if bytes[index]==b'%' {
   let Some(pair)=bytes.get(index+1..index+3) else{return Err(DeskError::InvalidPath("文档图片路径编码无效".into()))};
   let hex=std::str::from_utf8(pair).map_err(|_|DeskError::InvalidPath("文档图片路径编码无效".into()))?;
   decoded.push(u8::from_str_radix(hex,16).map_err(|_|DeskError::InvalidPath("文档图片路径编码无效".into()))?);index+=3;
  }else{decoded.push(bytes[index]);index+=1;}
 }
 String::from_utf8(decoded).map_err(|_|DeskError::InvalidPath("文档图片路径编码无效".into()))
}
/// Only serve managed assets, disallow path traversal and links escaping the asset directory.
pub fn read(data:&Path,reference:&str)->Result<(Vec<u8>,&'static str)>{
 let decoded=decode_percent_encoding(reference)?;let path=Path::new(&decoded);let mut parts=path.components();if parts.next()!=Some(Component::Normal("writing-assets".as_ref())) || !parts.clone().all(|c|matches!(c,Component::Normal(_))) || parts.next().is_none(){return Err(DeskError::InvalidPath("文档图片路径无效".into()))}
 let root=assets_directory(data)?;let file=data.join(path).canonicalize()?;if !file.starts_with(root)||!file.is_file(){return Err(DeskError::InvalidPath("文档图片不在资源目录内".into()))}
 let bytes=crate::contacts::import_photo(&file)?;let (_,mime)=format(&bytes)?;Ok((bytes,mime))
}
/// Export one Markdown document with a unique sibling folder for any managed images.
/// Unmanaged links remain unchanged; failures leave the original destination untouched.
pub fn export_markdown(data:&Path,page:&crate::library::Page,destination:&Path)->Result<()> {
 use std::collections::HashMap;
 let parent=destination.parent().filter(|p|p.is_dir()).ok_or_else(||DeskError::InvalidPath("导出目录不可用".into()))?;
 let mut output=String::new();let mut rest=page.body.as_str();let mut references:HashMap<String,String>=HashMap::new();
 let asset_name=format!("personal-desk-images-{}",uuid::Uuid::new_v4());
 let temp_assets=tempfile::Builder::new().prefix(".personaldesk-images-").tempdir_in(parent)?;
 while let Some(start)=rest.find("](writing-assets/") {
  let value_start=start+2;let Some(end_offset)=rest[value_start..].find(')') else {break};let end=value_start+end_offset;
  let reference=&rest[value_start..end];let replacement=if let Some(name)=references.get(reference){name.clone()}else{
   let (bytes,mime)=read(data,reference)?;let ext=match mime {"image/png"=>"png","image/jpeg"=>"jpg","image/webp"=>"webp","image/bmp"=>"bmp",_=>return Err(DeskError::UnsupportedFormat)};
   let name=format!("{}.{}",uuid::Uuid::new_v4(),ext);let mut out=fs::OpenOptions::new().write(true).create_new(true).open(temp_assets.path().join(&name))?;out.write_all(&bytes)?;out.sync_all()?;let relative=format!("{asset_name}/{name}");references.insert(reference.to_string(),relative.clone());relative
  };
  output.push_str(&rest[..value_start]);output.push_str(&replacement);output.push(')');rest=&rest[end+1..];
 }
 output.push_str(rest);
 let mut file=tempfile::NamedTempFile::new_in(parent)?;write!(file,"# {}\n\n{}",page.title,output)?;file.as_file().sync_all()?;
 let final_assets=parent.join(&asset_name);let mut moved_assets=false;
 if !references.is_empty(){fs::rename(temp_assets.path(),&final_assets)?;moved_assets=true;}
 if let Err(error)=file.persist(destination){if moved_assets{for value in references.values(){if let Some(name)=Path::new(value).file_name(){let _=fs::remove_file(final_assets.join(name));}}let _=fs::remove_dir(&final_assets);}return Err(error.error.into())}
 Ok(())
}

#[cfg(test)]
mod tests{
 use super::*;
 use image::{DynamicImage,ImageFormat,Rgba,RgbaImage};
 use std::io::Cursor;

 fn tiny_png()->Vec<u8>{
  let image=DynamicImage::ImageRgba8(RgbaImage::from_pixel(1,1,Rgba([20,40,60,255])));
  let mut bytes=Cursor::new(Vec::new());image.write_to(&mut bytes,ImageFormat::Png).unwrap();bytes.into_inner()
 }

 #[test]
 fn reads_legacy_asset_paths_with_percent_encoded_spaces(){
  let data=tempfile::tempdir().unwrap();let asset=data.path().join("writing-assets/obsidian-test/733aa4970b160f72");fs::create_dir_all(&asset).unwrap();
  let expected=tiny_png();fs::write(asset.join("Pasted image 20261016160000.png"),&expected).unwrap();
  let (actual,mime)=read(data.path(),"writing-assets/obsidian-test/733aa4970b160f72/Pasted%20image%2020261016160000.png").unwrap();
  assert_eq!(actual,expected);assert_eq!(mime,"image/png");
 }

 #[test]
 fn rejects_percent_encoded_path_traversal(){
  let data=tempfile::tempdir().unwrap();
  assert!(read(data.path(),"writing-assets/%2e%2e/outside.png").is_err());
  assert!(read(data.path(),"writing-assets/bad%XX.png").is_err());
 }
}
