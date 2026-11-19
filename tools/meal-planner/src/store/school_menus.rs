//! SQLite persistence for manually maintained school menus.
use crate::{MealPlannerError,MealPlannerStore,SchoolMenuDraft,SchoolMenuEntry,SchoolMenuStatus};
use chrono::NaiveDate;
use rusqlite::{OptionalExtension,params};
use std::time::{SystemTime,UNIX_EPOCH};

impl MealPlannerStore{
 pub fn save_school_menu_for_family(&self,family_id:&str,draft:SchoolMenuDraft)->Result<SchoolMenuEntry,MealPlannerError>{
  let draft=draft.validate().map_err(MealPlannerError::InvalidInput)?;let payload=serde_json::to_string(&draft)?;let now=timestamp();
  let c=self.connection.lock().map_err(|_|MealPlannerError::LockPoisoned)?;
  c.execute("INSERT INTO school_menus(family_id,date,status,payload,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5) ON CONFLICT(family_id,date) DO UPDATE SET status=excluded.status,payload=excluded.payload,updated_at=excluded.updated_at",params![family_id,draft.date.to_string(),draft.status.as_str(),payload,now])?;
  load_for_family(&c,family_id,draft.date)?.ok_or(MealPlannerError::NotFound)
 }
 pub fn get_school_menu_for_family(&self,family_id:&str,date:NaiveDate)->Result<Option<SchoolMenuEntry>,MealPlannerError>{let c=self.connection.lock().map_err(|_|MealPlannerError::LockPoisoned)?;load_for_family(&c,family_id,date)}
 pub fn list_school_menus_for_family(&self,family_id:&str,from:NaiveDate,to:NaiveDate)->Result<Vec<SchoolMenuEntry>,MealPlannerError>{
  if to<from{return Err(MealPlannerError::InvalidInput("school menu range ends before it starts".into()))}
  let c=self.connection.lock().map_err(|_|MealPlannerError::LockPoisoned)?;
  let mut st=c.prepare("SELECT id,date,status,payload,created_at,updated_at FROM school_menus WHERE family_id=?1 AND date>=?2 AND date<=?3 ORDER BY date")?;
  st.query_map(params![family_id,from.to_string(),to.to_string()],map_entry)?.collect::<Result<Vec<_>,_>>().map_err(Into::into)
 }
 pub fn delete_school_menu_for_family(&self,family_id:&str,date:NaiveDate)->Result<(),MealPlannerError>{let c=self.connection.lock().map_err(|_|MealPlannerError::LockPoisoned)?;if c.execute("DELETE FROM school_menus WHERE family_id=?1 AND date=?2",params![family_id,date.to_string()])?==0{return Err(MealPlannerError::NotFound)}Ok(())}
 // Legacy wrappers retained for the local household API/tests.
 pub fn save_school_menu(&self,draft:SchoolMenuDraft)->Result<SchoolMenuEntry,MealPlannerError>{self.save_school_menu_for_family("legacy",draft)}
 pub fn get_school_menu(&self,date:NaiveDate)->Result<Option<SchoolMenuEntry>,MealPlannerError>{self.get_school_menu_for_family("legacy",date)}
 pub fn list_school_menus(&self,from:NaiveDate,to:NaiveDate)->Result<Vec<SchoolMenuEntry>,MealPlannerError>{self.list_school_menus_for_family("legacy",from,to)}
 pub fn delete_school_menu(&self,date:NaiveDate)->Result<(),MealPlannerError>{self.delete_school_menu_for_family("legacy",date)}
}
fn load_for_family(c:&rusqlite::Connection,family_id:&str,date:NaiveDate)->Result<Option<SchoolMenuEntry>,MealPlannerError>{Ok(c.query_row("SELECT id,date,status,payload,created_at,updated_at FROM school_menus WHERE family_id=?1 AND date=?2",params![family_id,date.to_string()],map_entry).optional()?)}
fn map_entry(r:&rusqlite::Row<'_>)->rusqlite::Result<SchoolMenuEntry>{
 let date=NaiveDate::parse_from_str(&r.get::<_,String>(1)?,"%Y-%m-%d").map_err(|e|rusqlite::Error::FromSqlConversionFailure(1,rusqlite::types::Type::Text,Box::new(e)))?;
 let status=SchoolMenuStatus::parse(&r.get::<_,String>(2)?).ok_or_else(||rusqlite::Error::InvalidColumnType(2,"status".into(),rusqlite::types::Type::Text))?;
 let draft:SchoolMenuDraft=serde_json::from_str(&r.get::<_,String>(3)?).map_err(|e|rusqlite::Error::FromSqlConversionFailure(3,rusqlite::types::Type::Text,Box::new(e)))?;
 if draft.date!=date||draft.status!=status{return Err(rusqlite::Error::InvalidQuery)}
 Ok(SchoolMenuEntry{id:r.get(0)?,draft,created_at:r.get(4)?,updated_at:r.get(5)?})
}
fn timestamp()->i64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64}
