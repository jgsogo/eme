//! Shopping-list aggregation and deterministic reconciliation.
use crate::{MealAssignment,Recipe};
use serde::{Deserialize,Serialize};

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct ShoppingList{
 pub id:i64,pub family_id:String,pub start_date:chrono::NaiveDate,pub end_date:chrono::NaiveDate,
 pub items:Vec<ShoppingListItem>,pub created_at:i64,pub updated_at:i64
}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct ShoppingListItem{
 pub id:i64,pub name:String,pub quantity:Option<f64>,pub unit:Option<String>,pub purchased:bool,
 pub source_assignment_ids:Vec<i64>,pub source_recipe_ids:Vec<i64>,pub manually_added:bool,
 pub manually_removed:bool,pub quantity_overridden:bool,pub stale:bool
}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct NewShoppingListItem{pub name:String,pub quantity:Option<f64>,pub unit:Option<String>}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct ShoppingListItemUpdate{pub name:String,pub quantity:Option<f64>,pub unit:Option<String>,pub purchased:bool}
#[derive(Debug,Clone)]
pub(super) struct GeneratedItem{pub(super) name:String,pub(super) quantity:Option<f64>,pub(super) unit:Option<String>,pub(super) source_assignment_ids:Vec<i64>,pub(super) source_recipe_ids:Vec<i64>}

pub(super) fn generate_items(assignments:&[MealAssignment],mut recipe_for:impl FnMut(i64)->Option<Recipe>)->Result<Vec<GeneratedItem>,String>{
 let mut items:Vec<GeneratedItem>=Vec::new();
 for assignment in assignments{for recipe_id in &assignment.recipe_ids{
  let recipe=recipe_for(*recipe_id).ok_or_else(||format!("recipe {recipe_id} referenced by plan was not found"))?;
  let factor=assignment.participants.len() as f64/recipe.draft.servings as f64;
  for ingredient in recipe.draft.ingredients{
   let(unit,multiplier)=normalize_unit(ingredient.unit.as_deref());let quantity=ingredient.amount.map(|a|a*factor*multiplier);
   let key=(normalize_name(&ingredient.name),unit.clone());
   let existing=unit.as_ref().and_then(|_|items.iter_mut().find(|i|(normalize_name(&i.name),i.unit.clone())==key));
   if let Some(existing)=existing{existing.quantity=match(existing.quantity,quantity){(Some(a),Some(b))=>Some(a+b),_=>None};push_unique(&mut existing.source_assignment_ids,assignment.id);push_unique(&mut existing.source_recipe_ids,*recipe_id);}
   else{items.push(GeneratedItem{name:ingredient.name,quantity,unit,source_assignment_ids:vec![assignment.id],source_recipe_ids:vec![*recipe_id]});}
  }
 }}
 Ok(items)
}
fn normalize_name(v:&str)->String{v.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()}
fn normalize_unit(v:Option<&str>)->(Option<String>,f64){let Some(v)=v.map(str::trim).filter(|v|!v.is_empty())else{return(None,1.0)};match v.to_ascii_lowercase().as_str(){"g"|"gram"|"grams"=>(Some("g".into()),1.0),"kg"|"kilogram"|"kilograms"=>(Some("g".into()),1000.0),"ml"|"milliliter"|"milliliters"|"millilitre"|"millilitres"=>(Some("ml".into()),1.0),"l"|"liter"|"liters"|"litre"|"litres"=>(Some("ml".into()),1000.0),"pc"|"pcs"|"piece"|"pieces"|"unit"|"units"=>(Some("piece".into()),1.0),other=>(Some(other.to_owned()),1.0)}}
fn push_unique(v:&mut Vec<i64>,id:i64){if !v.contains(&id){v.push(id)}}
impl ShoppingList{
 pub(super) fn reconcile(existing:Option<Self>,family_id:String,start_date:chrono::NaiveDate,end_date:chrono::NaiveDate,generated:Vec<GeneratedItem>,now:i64)->Self{
  let(id,created_at,mut old)=existing.map(|l|(l.id,l.created_at,l.items)).unwrap_or((0,now,Vec::new()));let mut next=old.iter().map(|i|i.id).max().unwrap_or(0)+1;let mut items=Vec::new();
  for g in generated{
   let found=old.iter().position(|i|!i.manually_added&&normalize_name(&i.name)==normalize_name(&g.name)&&(i.manually_removed||i.quantity_overridden||normalize_unit(i.unit.as_deref()).0==normalize_unit(g.unit.as_deref()).0));
   if let Some(index)=found{let mut old_item=old.remove(index);old_item.source_assignment_ids=g.source_assignment_ids;old_item.source_recipe_ids=g.source_recipe_ids;old_item.stale=false;if !old_item.quantity_overridden{old_item.quantity=g.quantity;old_item.unit=g.unit;}items.push(old_item);}
   else{items.push(ShoppingListItem{id:next,name:g.name,quantity:g.quantity,unit:g.unit,purchased:false,source_assignment_ids:g.source_assignment_ids,source_recipe_ids:g.source_recipe_ids,manually_added:false,manually_removed:false,quantity_overridden:false,stale:false});next+=1;}
  }
  for mut item in old{if item.manually_added||item.manually_removed||item.purchased||item.quantity_overridden{if !item.manually_added{item.stale=true;item.source_assignment_ids.clear();item.source_recipe_ids.clear();}items.push(item)}}
  items.sort_by(|a,b|a.manually_added.cmp(&b.manually_added).then_with(||a.name.to_lowercase().cmp(&b.name.to_lowercase())).then_with(||a.id.cmp(&b.id)));
  Self{id,family_id,start_date,end_date,items,created_at,updated_at:now}
 }
}
impl NewShoppingListItem{pub(crate) fn validate(self)->Result<Self,String>{if self.name.trim().is_empty(){return Err("shopping-list item name must not be empty".into())}if self.quantity.is_some_and(|v|!v.is_finite()||v<=0.0){return Err("shopping-list quantity must be a positive finite number".into())}if self.unit.as_ref().is_some_and(|u|u.trim().is_empty()){return Err("shopping-list unit must not be empty".into())}Ok(Self{name:self.name.trim().into(),quantity:self.quantity,unit:self.unit.map(|u|u.trim().into())})}}
impl ShoppingListItemUpdate{pub(crate) fn validate(self)->Result<Self,String>{if self.name.trim().is_empty(){return Err("shopping-list item name must not be empty".into())}if self.quantity.is_some_and(|v|!v.is_finite()||v<=0.0){return Err("shopping-list quantity must be a positive finite number".into())}if self.unit.as_ref().is_some_and(|u|u.trim().is_empty()){return Err("shopping-list unit must not be empty".into())}Ok(Self{name:self.name.trim().into(),quantity:self.quantity,unit:self.unit.map(|u|u.trim().into()),purchased:self.purchased})}}
