//! SQLite repository operations for household members and participation.

use crate::{
    MealPlannerError, MealPlannerStore,
    domain::{
        FamilyMember, MealArrangement, MealException, MealParticipation, MealType, MealWeekday,
        RecurringParticipationRule, resolve_date, resolve_week,
    },
};
use chrono::NaiveDate;
use rusqlite::{OptionalExtension, params};

type ResolutionInputs = (
    Vec<FamilyMember>,
    Vec<RecurringParticipationRule>,
    Vec<MealException>,
);

impl MealPlannerStore {
    /// Refresh the local numeric member keys from the authoritative family
    /// membership list. External IDs are stable UserIds; names are snapshots.
    pub fn sync_family_members(
        &self,
        family_id: &str,
        members: &[(String, String)],
    ) -> Result<Vec<FamilyMember>, MealPlannerError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE family_members SET active = 0 WHERE family_id = ?1",
            [family_id],
        )?;
        for (external_id, name) in members {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            transaction.execute(
                "INSERT INTO family_members(family_id, external_id, name, active) VALUES (?1, ?2, ?3, 1)
                 ON CONFLICT(family_id, external_id) DO UPDATE SET name = excluded.name, active = 1",
                params![family_id, external_id, name],
            )?;
        }
        let mut statement = transaction.prepare(
            "SELECT id, name, active FROM family_members WHERE family_id = ?1 ORDER BY id",
        )?;
        let result = statement
            .query_map([family_id], |row| {
                Ok(FamilyMember {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    active: row.get::<_, i64>(2)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        transaction.commit()?;
        Ok(result)
    }

    pub fn add_recurring_rule_for_family(
        &self,
        family_id: &str,
        member_id: i64,
        weekday: MealWeekday,
        meal_type: MealType,
        arrangement: MealArrangement,
        effective_from: Option<NaiveDate>,
        effective_until: Option<NaiveDate>,
    ) -> Result<RecurringParticipationRule, MealPlannerError> {
        if effective_from
            .zip(effective_until)
            .is_some_and(|(a, b)| b < a)
        {
            return Err(MealPlannerError::InvalidInput(
                "recurring rule ends before it starts".into(),
            ));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        ensure_member_in_family(&connection, family_id, member_id)?;
        let effective_from = effective_from.map(|date| date.to_string());
        let effective_until = effective_until.map(|date| date.to_string());
        let existing_id = connection
            .query_row(
                "SELECT id FROM recurring_participation_rules
                 WHERE family_id=?1 AND member_id=?2 AND weekday=?3 AND meal_type=?4
                   AND effective_from IS ?5 AND effective_until IS ?6
                 ORDER BY id DESC LIMIT 1",
                params![
                    family_id,
                    member_id,
                    weekday as u8,
                    meal_type.as_str(),
                    effective_from,
                    effective_until
                ],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        let id = if let Some(id) = existing_id {
            connection.execute(
                "UPDATE recurring_participation_rules SET arrangement=?1 WHERE id=?2",
                params![arrangement.as_str(), id],
            )?;
            id
        } else {
            connection.execute(
                "INSERT INTO recurring_participation_rules(family_id,member_id,weekday,meal_type,arrangement,effective_from,effective_until)
                 VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![family_id, member_id, weekday as u8, meal_type.as_str(), arrangement.as_str(),
                    effective_from, effective_until],
            )?;
            connection.last_insert_rowid()
        };
        load_rule(&connection, id)?.ok_or(MealPlannerError::NotFound)
    }

    pub fn set_meal_exception_for_family(
        &self,
        family_id: &str,
        member_id: i64,
        date: NaiveDate,
        meal_type: MealType,
        arrangement: MealArrangement,
    ) -> Result<MealException, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        ensure_member_in_family(&connection, family_id, member_id)?;
        connection.execute(
            "INSERT INTO meal_participation_exceptions(family_id,member_id,date,meal_type,arrangement)
             VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(family_id,member_id,date,meal_type) DO UPDATE SET arrangement=excluded.arrangement",
            params![family_id, member_id, date.to_string(), meal_type.as_str(), arrangement.as_str()],
        )?;
        load_exception(&connection, member_id, date, meal_type)?.ok_or(MealPlannerError::NotFound)
    }

    pub fn create_family_member(&self, name: &str) -> Result<FamilyMember, MealPlannerError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(MealPlannerError::InvalidInput(
                "member name must not be empty".into(),
            ));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO family_members(family_id, external_id, name) VALUES ('legacy', ?1, ?1)",
            [name],
        )?;
        Ok(FamilyMember {
            id: connection.last_insert_rowid(),
            name: name.into(),
            active: true,
        })
    }
    pub fn rename_family_member(
        &self,
        id: i64,
        name: &str,
    ) -> Result<FamilyMember, MealPlannerError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(MealPlannerError::InvalidInput(
                "member name must not be empty".into(),
            ));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        if connection.execute(
            "UPDATE family_members SET name = ?1 WHERE id = ?2",
            params![name, id],
        )? == 0
        {
            return Err(MealPlannerError::NotFound);
        }
        load_member(&connection, id)?.ok_or(MealPlannerError::NotFound)
    }
    pub fn list_family_members_for_family(
        &self,
        family_id: &str,
    ) -> Result<Vec<FamilyMember>, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut statement = connection
            .prepare("SELECT id,name,active FROM family_members WHERE family_id=?1 ORDER BY id")?;
        statement
            .query_map([family_id], |row| {
                Ok(FamilyMember {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    active: row.get::<_, i64>(2)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_family_members(&self) -> Result<Vec<FamilyMember>, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut statement =
            connection.prepare("SELECT id, name, active FROM family_members ORDER BY id")?;
        statement
            .query_map([], |row| {
                Ok(FamilyMember {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    active: row.get::<_, i64>(2)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
    pub fn deactivate_family_member(&self, id: i64) -> Result<(), MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        if connection.execute("UPDATE family_members SET active = 0 WHERE id = ?1", [id])? == 0 {
            return Err(MealPlannerError::NotFound);
        }
        Ok(())
    }
    pub fn delete_family_member(&self, id: i64) -> Result<(), MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        if connection.execute("DELETE FROM family_members WHERE id = ?1", [id])? == 0 {
            return Err(MealPlannerError::NotFound);
        }
        Ok(())
    }
    pub fn add_recurring_rule(
        &self,
        member_id: i64,
        weekday: MealWeekday,
        meal_type: MealType,
        arrangement: MealArrangement,
        effective_from: Option<NaiveDate>,
        effective_until: Option<NaiveDate>,
    ) -> Result<RecurringParticipationRule, MealPlannerError> {
        if effective_from
            .zip(effective_until)
            .is_some_and(|(a, b)| b < a)
        {
            return Err(MealPlannerError::InvalidInput(
                "recurring rule ends before it starts".into(),
            ));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        ensure_member(&connection, member_id)?;
        connection.execute("INSERT INTO recurring_participation_rules(family_id, member_id, weekday, meal_type, arrangement, effective_from, effective_until) VALUES ('legacy', ?1, ?2, ?3, ?4, ?5, ?6)",
            params![member_id, weekday as u8, meal_type.as_str(), arrangement.as_str(), effective_from.map(|d| d.to_string()), effective_until.map(|d| d.to_string())])?;
        load_rule(&connection, connection.last_insert_rowid())?.ok_or(MealPlannerError::NotFound)
    }
    pub fn list_recurring_rules(
        &self,
        member_id: Option<i64>,
    ) -> Result<Vec<RecurringParticipationRule>, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut statement = connection.prepare("SELECT id, member_id, weekday, meal_type, arrangement, effective_from, effective_until FROM recurring_participation_rules WHERE (?1 IS NULL OR member_id = ?1) ORDER BY id")?;
        statement
            .query_map([member_id], map_rule)?
            .map(|row| {
                row.map_err(Into::into).and_then(|value| {
                    value.ok_or_else(|| {
                        MealPlannerError::InvalidInput("invalid recurring rule in database".into())
                    })
                })
            })
            .collect()
    }
    pub fn delete_recurring_rule(&self, id: i64) -> Result<(), MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        if connection.execute(
            "DELETE FROM recurring_participation_rules WHERE id = ?1",
            [id],
        )? == 0
        {
            return Err(MealPlannerError::NotFound);
        }
        Ok(())
    }
    pub fn set_meal_exception(
        &self,
        member_id: i64,
        date: NaiveDate,
        meal_type: MealType,
        arrangement: MealArrangement,
    ) -> Result<MealException, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        ensure_member(&connection, member_id)?;
        connection.execute("INSERT INTO meal_participation_exceptions(family_id, member_id, date, meal_type, arrangement) VALUES ('legacy', ?1, ?2, ?3, ?4) ON CONFLICT(family_id, member_id, date, meal_type) DO UPDATE SET arrangement = excluded.arrangement",
            params![member_id, date.to_string(), meal_type.as_str(), arrangement.as_str()])?;
        load_exception(&connection, member_id, date, meal_type)?.ok_or(MealPlannerError::NotFound)
    }
    pub fn delete_meal_exception(
        &self,
        member_id: i64,
        date: NaiveDate,
        meal_type: MealType,
    ) -> Result<(), MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        if connection.execute("DELETE FROM meal_participation_exceptions WHERE member_id = ?1 AND date = ?2 AND meal_type = ?3", params![member_id, date.to_string(), meal_type.as_str()])? == 0 { return Err(MealPlannerError::NotFound); }
        Ok(())
    }
    pub fn resolve_meal_date(
        &self,
        date: NaiveDate,
    ) -> Result<Vec<MealParticipation>, MealPlannerError> {
        let (members, rules, exceptions) = self.resolution_inputs()?;
        Ok(resolve_date(date, &members, &rules, &exceptions))
    }
    pub fn resolve_meal_week(
        &self,
        start: NaiveDate,
    ) -> Result<Vec<MealParticipation>, MealPlannerError> {
        let (members, rules, exceptions) = self.resolution_inputs()?;
        Ok(resolve_week(start, &members, &rules, &exceptions))
    }
    pub fn resolve_meal_week_for_family(
        &self,
        family_id: &str,
        start: NaiveDate,
    ) -> Result<Vec<MealParticipation>, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let members = {
            let mut statement = connection.prepare(
                "SELECT id, name, active FROM family_members WHERE family_id = ?1 ORDER BY id",
            )?;
            statement
                .query_map([family_id], |row| {
                    Ok(FamilyMember {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        active: row.get::<_, i64>(2)? != 0,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let rules = {
            let mut statement = connection.prepare(
                "SELECT id, member_id, weekday, meal_type, arrangement, effective_from, effective_until
                 FROM recurring_participation_rules WHERE family_id = ?1 ORDER BY id"
            )?;
            statement
                .query_map([family_id], map_rule)?
                .map(|row| {
                    row.map_err(Into::into).and_then(|value| {
                        value.ok_or_else(|| {
                            MealPlannerError::InvalidInput(
                                "invalid recurring rule in database".into(),
                            )
                        })
                    })
                })
                .collect::<Result<Vec<_>, MealPlannerError>>()?
        };
        let exceptions = {
            let mut statement = connection.prepare(
                "SELECT id, member_id, date, meal_type, arrangement
                 FROM meal_participation_exceptions WHERE family_id = ?1 ORDER BY id",
            )?;
            statement
                .query_map([family_id], map_exception)?
                .map(|row| {
                    row.map_err(Into::into).and_then(|value| {
                        value.ok_or_else(|| {
                            MealPlannerError::InvalidInput("invalid exception in database".into())
                        })
                    })
                })
                .collect::<Result<Vec<_>, MealPlannerError>>()?
        };
        Ok(resolve_week(start, &members, &rules, &exceptions))
    }

    fn resolution_inputs(&self) -> Result<ResolutionInputs, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let members = {
            let mut statement =
                connection.prepare("SELECT id, name, active FROM family_members ORDER BY id")?;
            statement
                .query_map([], |row| {
                    Ok(FamilyMember {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        active: row.get::<_, i64>(2)? != 0,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let rules = {
            let mut statement = connection.prepare("SELECT id, member_id, weekday, meal_type, arrangement, effective_from, effective_until FROM recurring_participation_rules ORDER BY id")?;
            statement
                .query_map([], map_rule)?
                .map(|row| {
                    row.map_err(Into::into).and_then(|v| {
                        v.ok_or_else(|| {
                            MealPlannerError::InvalidInput(
                                "invalid recurring rule in database".into(),
                            )
                        })
                    })
                })
                .collect::<Result<Vec<_>, MealPlannerError>>()?
        };
        let exceptions = {
            let mut statement = connection.prepare("SELECT id, member_id, date, meal_type, arrangement FROM meal_participation_exceptions ORDER BY id")?;
            statement
                .query_map([], map_exception)?
                .map(|row| {
                    row.map_err(Into::into).and_then(|v| {
                        v.ok_or_else(|| {
                            MealPlannerError::InvalidInput("invalid exception in database".into())
                        })
                    })
                })
                .collect::<Result<Vec<_>, MealPlannerError>>()?
        };
        Ok((members, rules, exceptions))
    }
}

fn ensure_member_in_family(
    connection: &rusqlite::Connection,
    family_id: &str,
    id: i64,
) -> Result<(), MealPlannerError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM family_members WHERE id=?1 AND family_id=?2 AND active=1)",
        params![id, family_id],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(MealPlannerError::NotFound)
    }
}
fn ensure_member(connection: &rusqlite::Connection, id: i64) -> Result<(), MealPlannerError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM family_members WHERE id = ?1)",
        [id],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(MealPlannerError::NotFound)
    }
}
fn load_member(
    connection: &rusqlite::Connection,
    id: i64,
) -> Result<Option<FamilyMember>, MealPlannerError> {
    Ok(connection
        .query_row(
            "SELECT id, name, active FROM family_members WHERE id = ?1",
            [id],
            |row| {
                Ok(FamilyMember {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    active: row.get::<_, i64>(2)? != 0,
                })
            },
        )
        .optional()?)
}
fn load_rule(
    connection: &rusqlite::Connection,
    id: i64,
) -> Result<Option<RecurringParticipationRule>, MealPlannerError> {
    Ok(connection.query_row("SELECT id, member_id, weekday, meal_type, arrangement, effective_from, effective_until FROM recurring_participation_rules WHERE id = ?1", [id], map_rule).optional()?.flatten())
}
fn load_exception(
    connection: &rusqlite::Connection,
    member_id: i64,
    date: NaiveDate,
    meal: MealType,
) -> Result<Option<MealException>, MealPlannerError> {
    Ok(connection.query_row("SELECT id, member_id, date, meal_type, arrangement FROM meal_participation_exceptions WHERE member_id = ?1 AND date = ?2 AND meal_type = ?3", params![member_id, date.to_string(), meal.as_str()], map_exception).optional()?.flatten())
}
fn map_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<RecurringParticipationRule>> {
    let weekday = MealWeekday::from_number(row.get(2)?);
    let meal = MealType::parse(&row.get::<_, String>(3)?);
    let arrangement = MealArrangement::parse(&row.get::<_, String>(4)?);
    let parse_date = |value: Option<String>| -> rusqlite::Result<Option<NaiveDate>> {
        value
            .map(|s| {
                NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })
            })
            .transpose()
    };
    let from = parse_date(row.get(5)?)?;
    let until = parse_date(row.get(6)?)?;
    if let (Some(weekday), Some(meal_type), Some(arrangement)) = (weekday, meal, arrangement) {
        Ok(Some(RecurringParticipationRule {
            id: row.get(0)?,
            member_id: row.get(1)?,
            weekday,
            meal_type,
            arrangement,
            effective_from: from,
            effective_until: until,
        }))
    } else {
        Ok(None)
    }
}
fn map_exception(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<MealException>> {
    let date = NaiveDate::parse_from_str(&row.get::<_, String>(2)?, "%Y-%m-%d").map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let meal = MealType::parse(&row.get::<_, String>(3)?);
    let arrangement = MealArrangement::parse(&row.get::<_, String>(4)?);
    if let (Some(meal_type), Some(arrangement)) = (meal, arrangement) {
        Ok(Some(MealException {
            id: row.get(0)?,
            member_id: row.get(1)?,
            date,
            meal_type,
            arrangement,
        }))
    } else {
        Ok(None)
    }
}
