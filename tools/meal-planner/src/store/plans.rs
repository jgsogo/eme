//! SQLite persistence for one mutable plan per family and week.
use crate::{
    MealPlannerError, MealPlannerStore,
    domain::{MealArrangement, ParticipationStatus},
    plans::{MealAssignment, PlanParticipant, WeeklyPlan, WeeklyPlanDraft},
};
use chrono::{NaiveDate, Utc};
use rusqlite::{OptionalExtension, params};

impl MealPlannerStore {
    pub fn create_weekly_plan_from_rules_for_family(
        &self,
        family_id: &str,
        week_start: NaiveDate,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let resolved = self.resolve_meal_week_for_family(family_id, week_start)?;
        let mut assignments: Vec<MealAssignment> = Vec::new();
        for entry in resolved {
            match entry.status {
                ParticipationStatus::Unconfigured => continue,
                ParticipationStatus::Configured {
                    arrangement: MealArrangement::NotParticipating,
                    ..
                } => continue,
                ParticipationStatus::Configured { .. } => {}
            }
            let participant = PlanParticipant {
                member_id: entry.member.id,
                name: entry.member.name,
            };
            if let Some(assignment) = assignments
                .iter_mut()
                .find(|a| a.date == entry.date && a.meal_type == entry.meal_type)
            {
                assignment.participants.push(participant)
            } else {
                let id = assignments.len() as i64 + 1;
                assignments.push(MealAssignment {
                    id,
                    date: entry.date,
                    meal_type: entry.meal_type,
                    participants: vec![participant],
                    recipe_ids: vec![],
                });
            }
        }
        self.create_weekly_plan_for_family(family_id, week_start, WeeklyPlanDraft { assignments })
    }
    pub fn create_weekly_plan_for_family(
        &self,
        family_id: &str,
        week_start: NaiveDate,
        draft: WeeklyPlanDraft,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let draft = draft
            .validate(week_start)
            .map_err(MealPlannerError::InvalidInput)?;
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        ensure_family_references(&c, family_id, &draft)?;
        let now = Utc::now().timestamp();
        let payload = serde_json::to_string(&draft.assignments)?;
        c.execute("INSERT INTO weekly_plans(family_id,week_start,assignments,created_at) VALUES(?1,?2,?3,?4)",params![family_id,week_start.to_string(),payload,now]).map_err(|e|{
            if matches!(e,rusqlite::Error::SqliteFailure(ref code,_) if code.code==rusqlite::ErrorCode::ConstraintViolation){MealPlannerError::Conflict("a plan already exists for this family and week".into())}else{MealPlannerError::Storage(e)}
        })?;
        load_plan(&c, c.last_insert_rowid())?.ok_or(MealPlannerError::NotFound)
    }
    /// Create or replace a family's weekly plan in one atomic operation.
    /// Repeating the same save is safe and does not create duplicate plans.
    pub fn save_weekly_plan_for_family(
        &self,
        family_id: &str,
        week_start: NaiveDate,
        draft: WeeklyPlanDraft,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let draft = draft
            .validate(week_start)
            .map_err(MealPlannerError::InvalidInput)?;
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        ensure_family_references(&c, family_id, &draft)?;
        let payload = serde_json::to_string(&draft.assignments)?;
        c.execute(
            "INSERT INTO weekly_plans(family_id,week_start,assignments,created_at)
             VALUES(?1,?2,?3,?4)
             ON CONFLICT(family_id,week_start) DO UPDATE SET assignments=excluded.assignments",
            params![family_id, week_start.to_string(), payload, Utc::now().timestamp()],
        )?;
        let id = c.query_row(
            "SELECT id FROM weekly_plans WHERE family_id=?1 AND week_start=?2",
            params![family_id, week_start.to_string()],
            |row| row.get(0),
        )?;
        load_plan(&c, id)?.ok_or(MealPlannerError::NotFound)
    }

    pub fn get_weekly_plan_for_family(
        &self,
        family_id: &str,
        week_start: NaiveDate,
    ) -> Result<Option<WeeklyPlan>, MealPlannerError> {
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let id = c
            .query_row(
                "SELECT id FROM weekly_plans WHERE family_id=?1 AND week_start=?2",
                params![family_id, week_start.to_string()],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;
        id.map(|id| load_plan(&c, id)?.ok_or(MealPlannerError::NotFound))
            .transpose()
    }
    pub fn list_weekly_plans_for_family(
        &self,
        family_id: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<WeeklyPlan>, MealPlannerError> {
        if to < from {
            return Err(MealPlannerError::InvalidInput(
                "plan range ends before it starts".into(),
            ));
        }
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut st=c.prepare("SELECT id FROM weekly_plans WHERE family_id=?1 AND week_start>=?2 AND week_start<=?3 ORDER BY week_start")?;
        let ids = st
            .query_map(params![family_id, from.to_string(), to.to_string()], |r| {
                r.get::<_, i64>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| load_plan(&c, id)?.ok_or(MealPlannerError::NotFound))
            .collect()
    }
    pub fn update_weekly_plan_for_family(
        &self,
        family_id: &str,
        week_start: NaiveDate,
        draft: WeeklyPlanDraft,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let draft = draft
            .validate(week_start)
            .map_err(MealPlannerError::InvalidInput)?;
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        ensure_family_references(&c, family_id, &draft)?;
        let payload = serde_json::to_string(&draft.assignments)?;
        if c.execute(
            "UPDATE weekly_plans SET assignments=?1 WHERE family_id=?2 AND week_start=?3",
            params![payload, family_id, week_start.to_string()],
        )? == 0
        {
            return Err(MealPlannerError::NotFound);
        }
        let id = c.query_row(
            "SELECT id FROM weekly_plans WHERE family_id=?1 AND week_start=?2",
            params![family_id, week_start.to_string()],
            |r| r.get(0),
        )?;
        load_plan(&c, id)?.ok_or(MealPlannerError::NotFound)
    }
    pub fn delete_weekly_plan_for_family(
        &self,
        family_id: &str,
        week_start: NaiveDate,
    ) -> Result<(), MealPlannerError> {
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        if c.execute(
            "DELETE FROM weekly_plans WHERE family_id=?1 AND week_start=?2",
            params![family_id, week_start.to_string()],
        )? == 0
        {
            return Err(MealPlannerError::NotFound);
        }
        Ok(())
    }
    pub fn plan_belongs_to_family(
        &self,
        plan_id: i64,
        family_id: &str,
    ) -> Result<bool, MealPlannerError> {
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        Ok(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM weekly_plans WHERE id=?1 AND family_id=?2)",
            params![plan_id, family_id],
            |r| r.get(0),
        )?)
    }
    // Compatibility helpers for the legacy local family used by store tests.
    pub fn create_weekly_plan(
        &self,
        week_start: NaiveDate,
        draft: WeeklyPlanDraft,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        self.create_weekly_plan_for_family("legacy", week_start, draft)
    }
    pub fn get_weekly_plan(
        &self,
        week_start: NaiveDate,
    ) -> Result<Option<WeeklyPlan>, MealPlannerError> {
        self.get_weekly_plan_for_family("legacy", week_start)
    }
    pub fn update_weekly_plan(
        &self,
        week_start: NaiveDate,
        draft: WeeklyPlanDraft,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        self.update_weekly_plan_for_family("legacy", week_start, draft)
    }
    pub fn create_weekly_plan_from_rules(
        &self,
        week_start: NaiveDate,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        self.create_weekly_plan_from_rules_for_family("legacy", week_start)
    }
}

fn ensure_family_references(
    c: &rusqlite::Connection,
    family_id: &str,
    draft: &WeeklyPlanDraft,
) -> Result<(), MealPlannerError> {
    for a in &draft.assignments {
        for p in &a.participants {
            let exists:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM family_members WHERE id=?1 AND family_id=?2 AND active=1)",params![p.member_id,family_id],|r|r.get(0))?;
            if !exists {
                return Err(MealPlannerError::NotFound);
            }
        }
        for id in &a.recipe_ids {
            let exists:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM recipes WHERE id=?1 AND (family_id IS NULL OR family_id=?2))",params![id,family_id],|r|r.get(0))?;
            if !exists {
                return Err(MealPlannerError::NotFound);
            }
        }
    }
    Ok(())
}

pub(super) fn load_plan(
    c: &rusqlite::Connection,
    id: i64,
) -> Result<Option<WeeklyPlan>, MealPlannerError> {
    Ok(c.query_row(
        "SELECT id,family_id,week_start,assignments,created_at FROM weekly_plans WHERE id=?1",
        [id],
        |r| {
            let date: String = r.get(2)?;
            let assignments: String = r.get(3)?;
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                date,
                assignments,
                r.get::<_, i64>(4)?,
            ))
        },
    )
    .optional()?
    .map(|(id, family_id, date, assignments, created_at)| {
        Ok::<WeeklyPlan, MealPlannerError>(WeeklyPlan {
            id,
            family_id,
            week_start: NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|e| {
                MealPlannerError::InvalidInput(format!("invalid plan date in database: {e}"))
            })?,
            assignments: serde_json::from_str(&assignments)?,
            created_at,
        })
    })
    .transpose()?)
}

#[cfg(test)]
mod save_tests {
    use super::*;
    use crate::{MealType, PlanParticipant};

    #[test]
    fn saving_a_plan_is_idempotent_and_preserves_empty_recipe_assignments() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let week = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let members = store
            .sync_family_members("family-a", &[("user-1".into(), "Alex".into())])
            .unwrap();
        let assignment = MealAssignment {
            id: 1,
            date: week,
            meal_type: MealType::Lunch,
            participants: vec![PlanParticipant {
                member_id: members[0].id,
                name: members[0].name.clone(),
            }],
            recipe_ids: vec![],
        };

        let first = store
            .save_weekly_plan_for_family(
                "family-a",
                week,
                WeeklyPlanDraft { assignments: vec![assignment.clone()] },
            )
            .unwrap();
        let second = store
            .save_weekly_plan_for_family(
                "family-a",
                week,
                WeeklyPlanDraft { assignments: vec![assignment] },
            )
            .unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(second.assignments.len(), 1);
        assert!(second.assignments[0].recipe_ids.is_empty());
        assert_eq!(
            store.get_weekly_plan_for_family("family-a", week).unwrap().unwrap().id,
            first.id
        );
        assert!(store.get_weekly_plan_for_family("family-b", week).unwrap().is_none());
    }
}
