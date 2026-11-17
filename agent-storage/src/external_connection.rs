//! Provider-independent persistence for external OAuth connections.
use std::{path::Path, str::FromStr, sync::Arc};
use async_trait::async_trait;
use chrono::{DateTime,Utc};
use ring::{aead::{Aad,AES_256_GCM,LessSafeKey,Nonce,UnboundKey},rand::{SecureRandom,SystemRandom}};
use rusqlite::{Connection,OptionalExtension,params};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::UserId;

const NONCE_LEN:usize=12;
#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash,Serialize,Deserialize)]
#[serde(transparent)]
pub struct ExternalConnectionId(Uuid);
impl ExternalConnectionId { pub fn new()->Self{Self(Uuid::new_v4())} }
impl Default for ExternalConnectionId {fn default()->Self{Self::new()}}
impl std::fmt::Display for ExternalConnectionId {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{self.0.fmt(f)}}
impl FromStr for ExternalConnectionId {type Err=uuid::Error;fn from_str(s:&str)->Result<Self,Self::Err>{Uuid::parse_str(s).map(Self)}}

#[derive(Clone)]
pub struct CredentialCipher{key:[u8;32]}
impl CredentialCipher{
 pub fn new(key:[u8;32])->Self{Self{key}}
 fn cipher(&self)->Result<LessSafeKey,ExternalConnectionStorageError>{UnboundKey::new(&AES_256_GCM,&self.key).map(LessSafeKey::new).map_err(|_|ExternalConnectionStorageError::Encryption)}
 /// Encrypts using AES-256-GCM; output is nonce || ciphertext || tag.
 pub fn encrypt(&self,plain:&[u8])->Result<Vec<u8>,ExternalConnectionStorageError>{
  let mut nonce=[0u8;NONCE_LEN];SystemRandom::new().fill(&mut nonce).map_err(|_|ExternalConnectionStorageError::Encryption)?;
  let mut data=plain.to_vec();self.cipher()?.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce),Aad::empty(),&mut data).map_err(|_|ExternalConnectionStorageError::Encryption)?;
  let mut out=nonce.to_vec();out.extend(data);Ok(out)
 }
 pub fn decrypt(&self,data:&[u8])->Result<Vec<u8>,ExternalConnectionStorageError>{
  if data.len()<NONCE_LEN+16{return Err(ExternalConnectionStorageError::Encryption)}
  let nonce:[u8;NONCE_LEN]=data[..NONCE_LEN].try_into().map_err(|_|ExternalConnectionStorageError::Encryption)?;
  let mut ciphertext=data[NONCE_LEN..].to_vec();
  self.cipher()?.open_in_place(Nonce::assume_unique_for_key(nonce),Aad::empty(),&mut ciphertext).map(|v|v.to_vec()).map_err(|_|ExternalConnectionStorageError::Encryption)
 }
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum ExternalConnectionStatus{Active,Revoked}
impl ExternalConnectionStatus{
 fn text(self)->&'static str{match self{Self::Active=>"active",Self::Revoked=>"revoked"}}
 fn parse(s:&str)->Result<Self,ExternalConnectionStorageError>{match s{"active"=>Ok(Self::Active),"revoked"=>Ok(Self::Revoked),_=>Err(ExternalConnectionStorageError::Backend(format!("invalid connection status: {s}")))}}
}
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
pub struct ExternalConnection{
 pub id:ExternalConnectionId,pub owner_id:UserId,pub provider:String,pub provider_subject:String,
 pub display_email:Option<String>,pub encrypted_refresh_token:Vec<u8>,pub granted_scopes:Vec<String>,
 pub status:ExternalConnectionStatus,pub created_at:DateTime<Utc>,pub updated_at:DateTime<Utc>,pub revoked_at:Option<DateTime<Utc>>,
}
pub struct NewExternalConnection{pub owner_id:UserId,pub provider:String,pub provider_subject:String,pub display_email:Option<String>,pub refresh_token:String,pub granted_scopes:Vec<String>}
pub struct NewExternalAuthorization{pub user_id:UserId,pub provider:String,pub state:String,pub pkce_verifier:String,pub requested_scopes:Vec<String>,pub expires_at:DateTime<Utc>}
pub struct ConsumedExternalAuthorization{pub id:Uuid,pub user_id:UserId,pub provider:String,pub pkce_verifier:String,pub requested_scopes:Vec<String>,pub created_at:DateTime<Utc>}

#[derive(Debug,Error)]
pub enum ExternalConnectionStorageError{
 #[error("external connection was not found")]NotFound,
 #[error("external connection already exists for this provider account")]AlreadyExists,
 #[error("pending authorization was not found")]AuthorizationNotFound,
 #[error("pending authorization has expired")]AuthorizationExpired,
 #[error("pending authorization has already been consumed")]AuthorizationConsumed,
 #[error("authorization provider does not match")]ProviderMismatch,
 #[error("required value is empty")]EmptyRequiredValue,
 #[error("refresh token must not be empty")]EmptyRefreshToken,
 #[error("credential encryption or authentication failed")]Encryption,
 #[error("external connection storage backend failed: {0}")]Backend(String),
}
#[async_trait]
pub trait ExternalConnectionStore:Send+Sync{
 async fn create_connection(&self,input:NewExternalConnection)->Result<ExternalConnection,ExternalConnectionStorageError>;
 async fn get_connection(&self,owner:UserId,id:ExternalConnectionId)->Result<Option<ExternalConnection>,ExternalConnectionStorageError>;
 async fn list_connections(&self,owner:UserId)->Result<Vec<ExternalConnection>,ExternalConnectionStorageError>;
 /// None preserves the existing encrypted refresh token.
 async fn update_credentials(&self,owner:UserId,id:ExternalConnectionId,token:Option<String>,scopes:Vec<String>)->Result<ExternalConnection,ExternalConnectionStorageError>;
 async fn revoke_connection(&self,owner:UserId,id:ExternalConnectionId)->Result<(),ExternalConnectionStorageError>;
 async fn begin_authorization(&self,input:NewExternalAuthorization)->Result<Uuid,ExternalConnectionStorageError>;
 /// Atomically consumes state and decrypts its PKCE verifier; a state succeeds once.
 async fn consume_authorization(&self,provider:&str,state:&str,now:DateTime<Utc>)->Result<ConsumedExternalAuthorization,ExternalConnectionStorageError>;
}
#[derive(Clone)]
pub struct SqliteExternalConnectionStore{db:Arc<Mutex<Connection>>,cipher:CredentialCipher}
impl SqliteExternalConnectionStore{
 pub fn open(path:impl AsRef<Path>,key:[u8;32])->Result<Self,rusqlite::Error>{
  let db=Connection::open(path)?;db.busy_timeout(std::time::Duration::from_secs(5))?;db.execute_batch("PRAGMA foreign_keys=ON;")?;
  db.execute_batch(include_str!("../migrations/0001_identity_and_families.sql"))?;db.execute_batch(include_str!("../migrations/0002_family_invitations.sql"))?;db.execute_batch(include_str!("../migrations/0003_external_connections.sql"))?;
  Ok(Self{db:Arc::new(Mutex::new(db)),cipher:CredentialCipher::new(key)})
 }
}
fn backend(e:rusqlite::Error)->ExternalConnectionStorageError{ExternalConnectionStorageError::Backend(e.to_string())}
fn time(s:&str)->Result<DateTime<Utc>,ExternalConnectionStorageError>{DateTime::parse_from_rfc3339(s).map(|v|v.with_timezone(&Utc)).map_err(|e|ExternalConnectionStorageError::Backend(e.to_string()))}
fn ensure_user(db:&Connection,id:UserId)->Result<(),rusqlite::Error>{let n=Utc::now().to_rfc3339();db.execute("INSERT OR IGNORE INTO users(id,display_name,created_at,updated_at) VALUES(?1,NULL,?2,?2)",params![id.to_string(),n])?;Ok(())}
fn scopes(v:&[String])->Result<String,ExternalConnectionStorageError>{serde_json::to_string(v).map_err(|e|ExternalConnectionStorageError::Backend(e.to_string()))}
fn parse_scopes(v:&str)->Result<Vec<String>,ExternalConnectionStorageError>{serde_json::from_str(v).map_err(|e|ExternalConnectionStorageError::Backend(e.to_string()))}
fn state_hash(s:&str)->String{Sha256::digest(s.as_bytes()).iter().map(|b|format!("{b:02x}")).collect()}
fn load(db:&Connection,owner:UserId,id:ExternalConnectionId)->Result<Option<ExternalConnection>,ExternalConnectionStorageError>{
 let row=db.query_row("SELECT provider,provider_subject,display_email,encrypted_refresh_token,granted_scopes,status,created_at,updated_at,revoked_at FROM external_connections WHERE owner_id=?1 AND id=?2",params![owner.to_string(),id.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,Vec<u8>>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,String>(7)?,r.get::<_,Option<String>>(8)?))).optional().map_err(backend)?;
 row.map(|(p,s,e,t,sc,st,c,u,r)|Ok(ExternalConnection{id,owner_id:owner,provider:p,provider_subject:s,display_email:e,encrypted_refresh_token:t,granted_scopes:parse_scopes(&sc)?,status:ExternalConnectionStatus::parse(&st)?,created_at:time(&c)?,updated_at:time(&u)?,revoked_at:r.map(|v|time(&v)).transpose()?})).transpose()
}
#[async_trait]
impl ExternalConnectionStore for SqliteExternalConnectionStore{
 async fn create_connection(&self,x:NewExternalConnection)->Result<ExternalConnection,ExternalConnectionStorageError>{
  if x.provider.trim().is_empty()||x.provider_subject.trim().is_empty(){return Err(ExternalConnectionStorageError::EmptyRequiredValue)}
  if x.refresh_token.is_empty(){return Err(ExternalConnectionStorageError::EmptyRefreshToken)}
  let token=self.cipher.encrypt(x.refresh_token.as_bytes())?;let scopes=scopes(&x.granted_scopes)?;let id=ExternalConnectionId::new();let now=Utc::now().to_rfc3339();let db=self.db.lock().await;ensure_user(&db,x.owner_id).map_err(backend)?;
  let result=db.execute("INSERT INTO external_connections(id,owner_id,provider,provider_subject,display_email,encrypted_refresh_token,granted_scopes,status,created_at,updated_at,revoked_at) VALUES(?1,?2,?3,?4,?5,?6,?7,'active',?8,?8,NULL)",params![id.to_string(),x.owner_id.to_string(),x.provider.trim(),x.provider_subject.trim(),x.display_email,token,scopes,now]);
  match result{Ok(_)=>load(&db,x.owner_id,id)?.ok_or(ExternalConnectionStorageError::NotFound),Err(rusqlite::Error::SqliteFailure(e,_))if e.code==rusqlite::ErrorCode::ConstraintViolation=>Err(ExternalConnectionStorageError::AlreadyExists),Err(e)=>Err(backend(e))}
 }
 async fn get_connection(&self,o:UserId,id:ExternalConnectionId)->Result<Option<ExternalConnection>,ExternalConnectionStorageError>{load(&self.db.lock().await,o,id)}
 async fn list_connections(&self,o:UserId)->Result<Vec<ExternalConnection>,ExternalConnectionStorageError>{
  let db=self.db.lock().await;let mut q=db.prepare("SELECT id FROM external_connections WHERE owner_id=?1 ORDER BY created_at,id").map_err(backend)?;
  let rows=q.query_map([o.to_string()],|r|r.get::<_,String>(0)).map_err(backend)?;let mut out=Vec::new();
  for row in rows{let id=ExternalConnectionId::from_str(&row.map_err(backend)?).map_err(|e|ExternalConnectionStorageError::Backend(e.to_string()))?;if let Some(v)=load(&db,o,id)?{out.push(v)}}Ok(out)
 }
 async fn update_credentials(&self,o:UserId,id:ExternalConnectionId,token:Option<String>,s:Vec<String>)->Result<ExternalConnection,ExternalConnectionStorageError>{
  let s=scopes(&s)?;let token=token.map(|v|if v.is_empty(){Err(ExternalConnectionStorageError::EmptyRefreshToken)}else{self.cipher.encrypt(v.as_bytes())}).transpose()?;let db=self.db.lock().await;let now=Utc::now().to_rfc3339();
  let n=if let Some(t)=token{db.execute("UPDATE external_connections SET encrypted_refresh_token=?1,granted_scopes=?2,updated_at=?3 WHERE owner_id=?4 AND id=?5 AND status='active'",params![t,s,now,o.to_string(),id.to_string()])}else{db.execute("UPDATE external_connections SET granted_scopes=?1,updated_at=?2 WHERE owner_id=?3 AND id=?4 AND status='active'",params![s,now,o.to_string(),id.to_string()])}.map_err(backend)?;
  if n==0{return Err(ExternalConnectionStorageError::NotFound)}load(&db,o,id)?.ok_or(ExternalConnectionStorageError::NotFound)
 }
 async fn revoke_connection(&self,o:UserId,id:ExternalConnectionId)->Result<(),ExternalConnectionStorageError>{
  let db=self.db.lock().await;let now=Utc::now().to_rfc3339();let n=db.execute("UPDATE external_connections SET status='revoked',revoked_at=?1,updated_at=?1 WHERE owner_id=?2 AND id=?3 AND status='active'",params![now,o.to_string(),id.to_string()]).map_err(backend)?;
  if n==0{Err(ExternalConnectionStorageError::NotFound)}else{Ok(())}
 }
 async fn begin_authorization(&self,x:NewExternalAuthorization)->Result<Uuid,ExternalConnectionStorageError>{
  if x.provider.trim().is_empty()||x.state.is_empty()||x.pkce_verifier.is_empty(){return Err(ExternalConnectionStorageError::EmptyRequiredValue)}
  let now=Utc::now();if x.expires_at<=now{return Err(ExternalConnectionStorageError::AuthorizationExpired)}
  let verifier=self.cipher.encrypt(x.pkce_verifier.as_bytes())?;let scopes=scopes(&x.requested_scopes)?;let id=Uuid::new_v4();let db=self.db.lock().await;ensure_user(&db,x.user_id).map_err(backend)?;
  db.execute("INSERT INTO pending_external_authorizations(id,user_id,provider,state_hash,encrypted_pkce_verifier,requested_scopes,created_at,expires_at,consumed_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,NULL)",params![id.to_string(),x.user_id.to_string(),x.provider.trim(),state_hash(&x.state),verifier,scopes,now.to_rfc3339(),x.expires_at.to_rfc3339()]).map_err(backend)?;Ok(id)
 }
 async fn consume_authorization(&self,p:&str,state:&str,now:DateTime<Utc>)->Result<ConsumedExternalAuthorization,ExternalConnectionStorageError>{
  if p.trim().is_empty()||state.is_empty(){return Err(ExternalConnectionStorageError::EmptyRequiredValue)}
  let mut db=self.db.lock().await;let tx=db.transaction().map_err(backend)?;
  let row=tx.query_row("SELECT id,user_id,provider,encrypted_pkce_verifier,requested_scopes,created_at,expires_at,consumed_at FROM pending_external_authorizations WHERE state_hash=?1",[state_hash(state)],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Vec<u8>>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,Option<String>>(7)?))).optional().map_err(backend)?.ok_or(ExternalConnectionStorageError::AuthorizationNotFound)?;
  let(id,user,provider,verifier,sc,created,expires,consumed)=row;if provider!=p{return Err(ExternalConnectionStorageError::ProviderMismatch)}
  if consumed.is_some(){return Err(ExternalConnectionStorageError::AuthorizationConsumed)}if time(&expires)?<=now{return Err(ExternalConnectionStorageError::AuthorizationExpired)}
  let n=tx.execute("UPDATE pending_external_authorizations SET consumed_at=?1 WHERE id=?2 AND consumed_at IS NULL",params![now.to_rfc3339(),id]).map_err(backend)?;if n!=1{return Err(ExternalConnectionStorageError::AuthorizationConsumed)}
  tx.commit().map_err(backend)?;let verifier=String::from_utf8(self.cipher.decrypt(&verifier)?).map_err(|_|ExternalConnectionStorageError::Encryption)?;
  Ok(ConsumedExternalAuthorization{id:Uuid::parse_str(&id).map_err(|e|ExternalConnectionStorageError::Backend(e.to_string()))?,user_id:UserId::from_str(&user).map_err(|e|ExternalConnectionStorageError::Backend(e.to_string()))?,provider,pkce_verifier:verifier,requested_scopes:parse_scopes(&sc)?,created_at:time(&created)?})
 }
}

#[cfg(test)]
mod tests{
 use super::*;use chrono::Duration;
 fn path()->std::path::PathBuf{std::env::temp_dir().join(format!("eme-ext-{}.sqlite",Uuid::new_v4()))}
 fn key()->[u8;32]{[0x42;32]}
 #[test]fn cipher_rejects_tampering(){let c=CredentialCipher::new(key());let mut v=c.encrypt(b"secret").unwrap();assert_eq!(c.decrypt(&v).unwrap(),b"secret");let i=v.len()-1;v[i]^=1;assert!(c.decrypt(&v).is_err());}
 #[tokio::test]async fn stores_multiple_accounts_and_preserves_refresh_token(){let p=path();let u=UserId::new();let s=SqliteExternalConnectionStore::open(&p,key()).unwrap();let a=s.create_connection(NewExternalConnection{owner_id:u,provider:"google".into(),provider_subject:"a".into(),display_email:None,refresh_token:"refresh-a".into(),granted_scopes:vec![]}).await.unwrap();s.create_connection(NewExternalConnection{owner_id:u,provider:"google".into(),provider_subject:"b".into(),display_email:None,refresh_token:"refresh-b".into(),granted_scopes:vec![]}).await.unwrap();let b=s.update_credentials(u,a.id,None,vec!["new-scope".into()]).await.unwrap();assert_eq!(a.encrypted_refresh_token,b.encrypted_refresh_token);assert_eq!(CredentialCipher::new(key()).decrypt(&b.encrypted_refresh_token).unwrap(),b"refresh-a");assert_eq!(s.list_connections(u).await.unwrap().len(),2);drop(s);let _=std::fs::remove_file(p);}
 #[tokio::test]async fn state_is_single_use_and_pkce_is_encrypted(){let p=path();let s=SqliteExternalConnectionStore::open(&p,key()).unwrap();let u=UserId::new();let id=s.begin_authorization(NewExternalAuthorization{user_id:u,provider:"google".into(),state:"random".into(),pkce_verifier:"secret-verifier".into(),requested_scopes:vec!["scope".into()],expires_at:Utc::now()+Duration::minutes(5)}).await.unwrap();let x=s.consume_authorization("google","random",Utc::now()).await.unwrap();assert_eq!(x.id,id);assert_eq!(x.pkce_verifier,"secret-verifier");assert!(matches!(s.consume_authorization("google","random",Utc::now()).await,Err(ExternalConnectionStorageError::AuthorizationConsumed)));drop(s);let _=std::fs::remove_file(p);}
}
