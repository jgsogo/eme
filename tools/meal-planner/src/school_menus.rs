//! Date-keyed school lunch menu domain model.
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum SchoolMenuStatus { Known, NotPublished, Closed, Unknown }
impl SchoolMenuStatus {
 pub(crate) fn as_str(self)->&'static str { match self {Self::Known=>"known",Self::NotPublished=>"not_published",Self::Closed=>"closed",Self::Unknown=>"unknown"} }
 pub(crate) fn parse(s:&str)->Option<Self>{match s {"known"=>Some(Self::Known),"not_published"=>Some(Self::NotPublished),"closed"=>Some(Self::Closed),"unknown"=>Some(Self::Unknown),_=>None}}
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchoolMenuItem { pub name:String, pub notes:Option<String>, pub allergens:Option<Vec<String>>, pub dietary_alternatives:Vec<String> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchoolMenuDraft { pub date:NaiveDate, pub status:SchoolMenuStatus, pub items:Vec<SchoolMenuItem>, pub notes:Option<String> }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum SchoolMenuOrigin { Manual, Import }
impl SchoolMenuOrigin {
 pub(crate) fn as_str(self)->&'static str {match self {Self::Manual=>"manual",Self::Import=>"import"}}
 pub(crate) fn parse(s:&str)->Option<Self>{match s {"manual"=>Some(Self::Manual),"import"=>Some(Self::Import),_=>None}}
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchoolMenuEntry {
 pub id:i64, #[serde(flatten)] pub draft:SchoolMenuDraft, pub source:Option<String>,
 pub imported_at:Option<i64>, pub origin:SchoolMenuOrigin, pub created_at:i64, pub updated_at:i64
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchoolMenuImportReport {pub inserted:usize,pub updated:usize,pub skipped_manual:usize}
impl SchoolMenuDraft {
 pub(crate) fn validate(mut self)->Result<Self,String>{
  match self.status {
   SchoolMenuStatus::Known if self.items.is_empty()=>return Err("a known school menu must contain at least one item".into()),
   SchoolMenuStatus::NotPublished|SchoolMenuStatus::Closed|SchoolMenuStatus::Unknown if !self.items.is_empty()=>return Err("only a known school menu may contain menu items".into()),
   _=>{}
  }
  for item in &mut self.items {
   item.name=item.name.trim().to_owned(); if item.name.is_empty(){return Err("school menu item name must not be empty".into())}
   item.notes=clean(item.notes.take()); item.allergens=item.allergens.take().map(normalize);
   item.dietary_alternatives=normalize(std::mem::take(&mut item.dietary_alternatives));
  }
  self.notes=clean(self.notes.take()); Ok(self)
 }
}
fn clean(v:Option<String>)->Option<String>{v.map(|s|s.trim().to_owned()).filter(|s|!s.is_empty())}
fn normalize(v:Vec<String>)->Vec<String>{let mut out=Vec::new();for s in v{let s=s.trim();if !s.is_empty()&&!out.iter().any(|x:&String|x.eq_ignore_ascii_case(s)){out.push(s.to_owned())}}out}
#[cfg(test)] mod tests{
 use super::*;
 fn draft(status:SchoolMenuStatus,items:Vec<SchoolMenuItem>)->SchoolMenuDraft{SchoolMenuDraft{date:NaiveDate::from_ymd_opt(2026,11,16).unwrap(),status,items,notes:None}}
 fn item(name:&str)->SchoolMenuItem{SchoolMenuItem{name:name.into(),notes:None,allergens:None,dietary_alternatives:vec![]}}
 #[test] fn states_are_explicit_and_validated(){assert!(draft(SchoolMenuStatus::Known,vec![]).validate().is_err());assert!(draft(SchoolMenuStatus::Closed,vec![item("Soup")]).validate().is_err());for s in [SchoolMenuStatus::Unknown,SchoolMenuStatus::NotPublished,SchoolMenuStatus::Closed]{assert!(draft(s,vec![]).validate().is_ok())}}
 #[test] fn annotations_are_normalized(){let v=draft(SchoolMenuStatus::Known,vec![SchoolMenuItem{name:" Soup ".into(),notes:None,allergens:Some(vec!["gluten".into()," Gluten ".into()]),dietary_alternatives:vec![]}]).validate().unwrap();assert_eq!(v.items[0].name,"Soup");assert_eq!(v.items[0].allergens,Some(vec!["gluten".into()]))}
}