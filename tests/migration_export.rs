use personal_desk::{db::Database,export::{self,Scope},model::Credential};
#[test]
fn export_all_contains_temporary_data_and_preserves_source() {
 let root=tempfile::tempdir().unwrap();let dbdir=root.path().join("data");std::fs::create_dir(&dbdir).unwrap();let mut db=Database::open(&dbdir).unwrap();let c=Credential{id:"sample".into(),title:"测试账号".into(),..Default::default()};db.save_credential(&c).unwrap();drop(db);
 let target=root.path().join("export.zip");let report=export::write(&dbdir,&target,Scope::All).unwrap();assert_eq!(report.path,target);assert!(target.metadata().unwrap().len()>0);let db=Database::open(&dbdir).unwrap();assert_eq!(db.credentials().unwrap()[0].title,"测试账号");
}
