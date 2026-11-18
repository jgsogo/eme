//! SQLite persistence for date-keyed school lunch menus.
use crate::{
    MealPlannerError, MealPlannerStore, SchoolMenuDraft, SchoolMenuEntry, SchoolMenuImportReport,
    SchoolMenuOrigin, SchoolMenuStatus,
};
use chrono::NaiveDate;
use rusqlite::{OptionalExtension, params};
use std::time::{SystemTime, UNIX_EPOCH};
impl MealPlannerStore {
    pub fn save_school_menu(
        &self,
        draft: SchoolMenuDraft,
    ) -> Result<SchoolMenuEntry, MealPlannerError> {
        let draft = draft.validate().map_err(MealPlannerError::InvalidInput)?;
        let payload = serde_json::to_string(&draft)?;
        let now = timestamp();
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        c.execute("INSERT INTO school_menus(family_id,date,status,payload,source,imported_at,origin,created_at,updated_at) VALUES('legacy',?1,?2,?3,NULL,NULL,'manual',?4,?4) ON CONFLICT(family_id,date) DO UPDATE SET status=excluded.status,payload=excluded.payload,source=NULL,imported_at=NULL,origin='manual',updated_at=excluded.updated_at",params![draft.date.to_string(),draft.status.as_str(),payload,now])?;
        load(&c, draft.date)?.ok_or(MealPlannerError::NotFound)
    }
    pub fn import_school_menus(
        &self,
        source: &str,
        drafts: Vec<SchoolMenuDraft>,
    ) -> Result<SchoolMenuImportReport, MealPlannerError> {
        let source = source.trim();
        if source.is_empty() {
            return Err(MealPlannerError::InvalidInput(
                "import source must not be empty".into(),
            ));
        }
        let mut valid = Vec::new();
        let mut dates = std::collections::HashSet::new();
        for d in drafts {
            let d = d.validate().map_err(MealPlannerError::InvalidInput)?;
            if !dates.insert(d.date) {
                return Err(MealPlannerError::InvalidInput(
                    "an import batch contains duplicate dates".into(),
                ));
            }
            valid.push(d)
        }
        let now = timestamp();
        let mut c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let tx = c.transaction()?;
        let mut report = SchoolMenuImportReport {
            inserted: 0,
            updated: 0,
            skipped_manual: 0,
        };
        for d in valid {
            let old: Option<String> = tx
                .query_row(
                    "SELECT origin FROM school_menus WHERE family_id='legacy' AND date=?1",
                    [d.date.to_string()],
                    |r| r.get(0),
                )
                .optional()?;
            if old.as_deref() == Some("manual") {
                report.skipped_manual += 1;
                continue;
            }
            let payload = serde_json::to_string(&d)?;
            tx.execute("INSERT INTO school_menus(family_id,date,status,payload,source,imported_at,origin,created_at,updated_at) VALUES('legacy',?1,?2,?3,?4,?5,'import',?5,?5) ON CONFLICT(family_id,date) DO UPDATE SET status=excluded.status,payload=excluded.payload,source=excluded.source,imported_at=excluded.imported_at,origin='import',updated_at=excluded.updated_at",params![d.date.to_string(),d.status.as_str(),payload,source,now])?;
            if old.is_some() {
                report.updated += 1
            } else {
                report.inserted += 1
            }
        }
        tx.commit()?;
        Ok(report)
    }
    pub fn get_school_menu(
        &self,
        date: NaiveDate,
    ) -> Result<Option<SchoolMenuEntry>, MealPlannerError> {
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        load(&c, date)
    }
    pub fn list_school_menus(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<SchoolMenuEntry>, MealPlannerError> {
        if to < from {
            return Err(MealPlannerError::InvalidInput(
                "school menu range ends before it starts".into(),
            ));
        }
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut s=c.prepare("SELECT id,date,status,payload,source,imported_at,origin,created_at,updated_at FROM school_menus WHERE family_id='legacy' AND date>=?1 AND date<=?2 ORDER BY date")?;
        s.query_map(params![from.to_string(), to.to_string()], map_entry)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
    pub fn delete_school_menu(&self, date: NaiveDate) -> Result<(), MealPlannerError> {
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        if c.execute("DELETE FROM school_menus WHERE family_id='legacy' AND date=?1", [date.to_string()])? == 0 {
            return Err(MealPlannerError::NotFound);
        }
        Ok(())
    }

    pub fn save_school_menu_for_family(
        &self, family_id: &str, draft: SchoolMenuDraft,
    ) -> Result<SchoolMenuEntry, MealPlannerError> {
        let draft = draft.validate().map_err(MealPlannerError::InvalidInput)?;
        let payload = serde_json::to_string(&draft)?;
        let now = timestamp();
        let connection = self.connection.lock().map_err(|_| MealPlannerError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO school_menus(family_id,date,status,payload,source,imported_at,origin,created_at,updated_at)
             VALUES(?1,?2,?3,?4,NULL,NULL,'manual',?5,?5)
             ON CONFLICT(family_id,date) DO UPDATE SET status=excluded.status,payload=excluded.payload,
             source=NULL,imported_at=NULL,origin='manual',updated_at=excluded.updated_at",
            params![family_id, draft.date.to_string(), draft.status.as_str(), payload, now],
        )?;
        load_for_family(&connection, family_id, draft.date)?.ok_or(MealPlannerError::NotFound)
    }

    pub fn get_school_menu_for_family(
        &self, family_id: &str, date: NaiveDate,
    ) -> Result<Option<SchoolMenuEntry>, MealPlannerError> {
        let connection = self.connection.lock().map_err(|_| MealPlannerError::LockPoisoned)?;
        load_for_family(&connection, family_id, date)
    }

    pub fn list_school_menus_for_family(
        &self, family_id: &str, from: NaiveDate, to: NaiveDate,
    ) -> Result<Vec<SchoolMenuEntry>, MealPlannerError> {
        if to < from { return Err(MealPlannerError::InvalidInput("school menu range ends before it starts".into())); }
        let connection = self.connection.lock().map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT id,date,status,payload,source,imported_at,origin,created_at,updated_at
             FROM school_menus WHERE family_id=?1 AND date>=?2 AND date<=?3 ORDER BY date"
        )?;
        statement.query_map(params![family_id, from.to_string(), to.to_string()], map_entry)?
            .collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_school_menu_for_family(
        &self, family_id: &str, date: NaiveDate,
    ) -> Result<(), MealPlannerError> {
        let connection = self.connection.lock().map_err(|_| MealPlannerError::LockPoisoned)?;
        if connection.execute("DELETE FROM school_menus WHERE family_id=?1 AND date=?2",
            params![family_id, date.to_string()])? == 0 { return Err(MealPlannerError::NotFound); }
        Ok(())
    }

    pub fn import_school_menus_for_family(
        &self, family_id: &str, source: &str, drafts: Vec<SchoolMenuDraft>,
    ) -> Result<SchoolMenuImportReport, MealPlannerError> {
        let source = source.trim();
        if source.is_empty() { return Err(MealPlannerError::InvalidInput("import source must not be empty".into())); }
        let mut valid = Vec::new();
        let mut dates = std::collections::HashSet::new();
        for draft in drafts {
            let draft = draft.validate().map_err(MealPlannerError::InvalidInput)?;
            if !dates.insert(draft.date) { return Err(MealPlannerError::InvalidInput("an import batch contains duplicate dates".into())); }
            valid.push(draft);
        }
        let now = timestamp();
        let mut connection = self.connection.lock().map_err(|_| MealPlannerError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let mut report = SchoolMenuImportReport { inserted: 0, updated: 0, skipped_manual: 0 };
        for draft in valid {
            let old: Option<String> = transaction.query_row(
                "SELECT origin FROM school_menus WHERE family_id=?1 AND date=?2",
                params![family_id, draft.date.to_string()], |row| row.get(0)
            ).optional()?;
            if old.as_deref() == Some("manual") { report.skipped_manual += 1; continue; }
            let payload = serde_json::to_string(&draft)?;
            transaction.execute(
                "INSERT INTO school_menus(family_id,date,status,payload,source,imported_at,origin,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,'import',?6,?6)
                 ON CONFLICT(family_id,date) DO UPDATE SET status=excluded.status,payload=excluded.payload,
                 source=excluded.source,imported_at=excluded.imported_at,origin='import',updated_at=excluded.updated_at",
                params![family_id, draft.date.to_string(), draft.status.as_str(), payload, source, now],
            )?;
            if old.is_some() { report.updated += 1 } else { report.inserted += 1 }
        }
        transaction.commit()?;
        Ok(report)
    }
}
fn load_for_family(
    connection: &rusqlite::Connection, family_id: &str, date: NaiveDate,
) -> Result<Option<SchoolMenuEntry>, MealPlannerError> {
    Ok(connection.query_row(
        "SELECT id,date,status,payload,source,imported_at,origin,created_at,updated_at
         FROM school_menus WHERE family_id=?1 AND date=?2",
        params![family_id, date.to_string()], map_entry
    ).optional()?)
}
fn load(
    c: &rusqlite::Connection,
    date: NaiveDate,
) -> Result<Option<SchoolMenuEntry>, MealPlannerError> {
    Ok(c.query_row("SELECT id,date,status,payload,source,imported_at,origin,created_at,updated_at FROM school_menus WHERE family_id='legacy' AND date=?1",[date.to_string()],map_entry).optional()?)
}
fn map_entry(r: &rusqlite::Row<'_>) -> rusqlite::Result<SchoolMenuEntry> {
    let date = NaiveDate::parse_from_str(&r.get::<_, String>(1)?, "%Y-%m-%d").map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let status = SchoolMenuStatus::parse(&r.get::<_, String>(2)?).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(2, "status".into(), rusqlite::types::Type::Text)
    })?;
    let origin = SchoolMenuOrigin::parse(&r.get::<_, String>(6)?).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(6, "origin".into(), rusqlite::types::Type::Text)
    })?;
    let draft: SchoolMenuDraft = serde_json::from_str(&r.get::<_, String>(3)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    if draft.date != date || draft.status != status {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(SchoolMenuEntry {
        id: r.get(0)?,
        draft,
        source: r.get(4)?,
        imported_at: r.get(5)?,
        origin,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}
fn timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::SchoolMenuItem;
    fn store() -> MealPlannerStore {
        MealPlannerStore::open_in_memory().unwrap()
    }
    fn date(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }
    fn menu(day: u32, status: SchoolMenuStatus, name: &str) -> SchoolMenuDraft {
        SchoolMenuDraft {
            date: date(day),
            status,
            items: if name.is_empty() {
                vec![]
            } else {
                vec![SchoolMenuItem {
                    name: name.into(),
                    notes: None,
                    allergens: None,
                    dietary_alternatives: vec![],
                }]
            },
            notes: None,
        }
    }
    #[test]
    fn absent_unknown_closed_and_unpublished_are_distinct() {
        let s = store();
        assert!(s.get_school_menu(date(16)).unwrap().is_none());
        for (d, status) in [
            (16, SchoolMenuStatus::Unknown),
            (17, SchoolMenuStatus::NotPublished),
            (18, SchoolMenuStatus::Closed),
        ] {
            s.save_school_menu(menu(d, status, "")).unwrap();
        }
        let v = s.list_school_menus(date(16), date(18)).unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].draft.status, SchoolMenuStatus::Unknown);
        assert_eq!(v[1].draft.status, SchoolMenuStatus::NotPublished);
        assert_eq!(v[2].draft.status, SchoolMenuStatus::Closed);
        assert!(s.list_school_menus(date(19), date(20)).unwrap().is_empty())
    }
    #[test]
    fn imports_keep_provenance_and_never_overwrite_manual_entries() {
        let s = store();
        s.save_school_menu(menu(16, SchoolMenuStatus::Known, "Manual soup"))
            .unwrap();
        let r = s
            .import_school_menus(
                "Provider",
                vec![
                    menu(16, SchoolMenuStatus::Known, "Imported soup"),
                    menu(17, SchoolMenuStatus::Known, "Pasta"),
                ],
            )
            .unwrap();
        assert_eq!(
            r,
            SchoolMenuImportReport {
                inserted: 1,
                updated: 0,
                skipped_manual: 1
            }
        );
        let m = s.get_school_menu(date(16)).unwrap().unwrap();
        assert_eq!(m.draft.items[0].name, "Manual soup");
        assert_eq!(m.origin, SchoolMenuOrigin::Manual);
        let i = s.get_school_menu(date(17)).unwrap().unwrap();
        assert_eq!(i.source.as_deref(), Some("Provider"));
        assert!(i.imported_at.is_some());
        let r = s
            .import_school_menus(
                "Provider v2",
                vec![menu(17, SchoolMenuStatus::Known, "Updated")],
            )
            .unwrap();
        assert_eq!(r.updated, 1);
        assert_eq!(
            s.get_school_menu(date(17)).unwrap().unwrap().draft.items[0].name,
            "Updated"
        )
    }
    #[test]
    fn rejects_invalid_range_and_duplicate_dates() {
        let s = store();
        assert!(s.list_school_menus(date(18), date(16)).is_err());
        assert!(
            s.import_school_menus(
                "src",
                vec![
                    menu(16, SchoolMenuStatus::Unknown, ""),
                    menu(16, SchoolMenuStatus::Unknown, "")
                ]
            )
            .is_err()
        )
    }
}
