//! SQLite persistence for weekly plans and immutable confirmed revisions.
use crate::{
    MealPlannerError, MealPlannerStore,
    domain::{MealArrangement, ParticipationStatus},
    plans::{MealAssignment, PlanParticipant, WeeklyPlan, WeeklyPlanDraft, WeeklyPlanStatus},
};
use chrono::{Datelike, NaiveDate, Utc, Weekday};
use rusqlite::{OptionalExtension, params};

impl MealPlannerStore {
    pub fn create_weekly_plan_from_rules_for_family(
        &self,
        family_id: &str,
        week_start: NaiveDate,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let resolved = self.resolve_meal_week_for_family(family_id, week_start)?;
        let mut assignments: Vec<MealAssignment> = Vec::new();
        for participation in resolved {
            let arrangement = match participation.status {
                ParticipationStatus::Unconfigured => continue,
                ParticipationStatus::Configured { arrangement, .. } => arrangement,
            };
            let participant = PlanParticipant {
                member_id: participation.member.id,
                name: participation.member.name,
            };
            if let Some(assignment) = assignments.iter_mut().find(|item| {
                item.date == participation.date
                    && item.meal_type == participation.meal_type
                    && item.arrangement == arrangement
            }) {
                assignment.participants.push(participant);
            } else {
                let id = assignments.len() as i64 + 1;
                assignments.push(MealAssignment {
                    id,
                    date: participation.date,
                    meal_type: participation.meal_type,
                    participants: vec![participant],
                    arrangement,
                    recipe_ids: vec![],
                    school_menu_date: (arrangement == MealArrangement::School)
                        .then_some(participation.date),
                    notes: None,
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
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        ensure_family_references(&connection, family_id, &draft)?;
        let revision: i64 = connection.query_row(
            "SELECT COALESCE(MAX(revision),0)+1 FROM weekly_plans WHERE family_id=?1 AND week_start=?2",
            params![family_id, week_start.to_string()], |row| row.get(0)
        )?;
        let now = Utc::now().timestamp();
        let assignments = serde_json::to_string(&draft.assignments)?;
        connection.execute(
            "INSERT INTO weekly_plans(family_id,week_start,revision,status,assignments,created_at) VALUES(?1,?2,?3,'draft',?4,?5)",
            params![family_id, week_start.to_string(), revision, assignments, now]
        )?;
        load_plan(&connection, connection.last_insert_rowid())?.ok_or(MealPlannerError::NotFound)
    }

    pub fn get_weekly_plan_for_family(
        &self,
        family_id: &str,
        week_start: NaiveDate,
    ) -> Result<Option<WeeklyPlan>, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let id = connection.query_row(
            "SELECT id FROM weekly_plans WHERE family_id=?1 AND week_start=?2 ORDER BY revision DESC LIMIT 1",
            params![family_id, week_start.to_string()], |row| row.get::<_, i64>(0)
        ).optional()?;
        match id {
            Some(id) => load_plan(&connection, id),
            None => Ok(None),
        }
    }

    pub fn list_weekly_plan_revisions_for_family(
        &self,
        family_id: &str,
        week_start: NaiveDate,
    ) -> Result<Vec<WeeklyPlan>, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut statement = connection.prepare("SELECT id FROM weekly_plans WHERE family_id=?1 AND week_start=?2 ORDER BY revision DESC")?;
        let ids = statement
            .query_map(params![family_id, week_start.to_string()], |row| {
                row.get::<_, i64>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| load_plan(&connection, id)?.ok_or(MealPlannerError::NotFound))
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
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        ensure_family_references(&connection, family_id, &draft)?;
        let id = latest_plan_id_for_family(&connection, family_id, week_start)?
            .ok_or(MealPlannerError::NotFound)?;
        let status: String = connection.query_row(
            "SELECT status FROM weekly_plans WHERE id=?1 AND family_id=?2",
            params![id, family_id],
            |row| row.get(0),
        )?;
        if status != "draft" {
            return Err(MealPlannerError::Conflict(
                "confirmed plans are immutable; create a revision first".into(),
            ));
        }
        let assignments = serde_json::to_string(&draft.assignments)?;
        connection.execute(
            "UPDATE weekly_plans SET assignments=?1 WHERE id=?2 AND family_id=?3",
            params![assignments, id, family_id],
        )?;
        load_plan(&connection, id)?.ok_or(MealPlannerError::NotFound)
    }

    pub fn confirm_weekly_plan_for_family(
        &self,
        family_id: &str,
        week_start: NaiveDate,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let id = latest_plan_id_for_family(&connection, family_id, week_start)?
            .ok_or(MealPlannerError::NotFound)?;
        let now = Utc::now().timestamp();
        let changed = connection.execute("UPDATE weekly_plans SET status='confirmed',confirmed_at=?1 WHERE id=?2 AND family_id=?3 AND status='draft'", params![now, id, family_id])?;
        if changed == 0 {
            return Err(MealPlannerError::Conflict(
                "only a draft plan can be confirmed".into(),
            ));
        }
        load_plan(&connection, id)?.ok_or(MealPlannerError::NotFound)
    }

    pub fn revise_weekly_plan_for_family(
        &self,
        family_id: &str,
        week_start: NaiveDate,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let id = latest_plan_id_for_family(&connection, family_id, week_start)?
            .ok_or(MealPlannerError::NotFound)?;
        let source = load_plan(&connection, id)?.ok_or(MealPlannerError::NotFound)?;
        if source.status != WeeklyPlanStatus::Confirmed {
            return Err(MealPlannerError::Conflict(
                "only a confirmed plan can be revised".into(),
            ));
        }
        let revision: i64 = connection.query_row(
            "SELECT COALESCE(MAX(revision),0)+1 FROM weekly_plans WHERE family_id=?1 AND week_start=?2",
            params![family_id, week_start.to_string()], |row| row.get(0)
        )?;
        let now = Utc::now().timestamp();
        let assignments = serde_json::to_string(&source.assignments)?;
        connection.execute(
            "INSERT INTO weekly_plans(family_id,week_start,revision,status,assignments,created_at) VALUES(?1,?2,?3,'draft',?4,?5)",
            params![family_id, week_start.to_string(), revision, assignments, now]
        )?;
        load_plan(&connection, connection.last_insert_rowid())?.ok_or(MealPlannerError::NotFound)
    }

    pub fn plan_belongs_to_family(
        &self,
        plan_id: i64,
        family_id: &str,
    ) -> Result<bool, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        Ok(connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM weekly_plans WHERE id=?1 AND family_id=?2)",
            params![plan_id, family_id],
            |row| row.get(0),
        )?)
    }

    /// Create a draft using the current household participation rules.
    /// Resolved participants are snapshotted into the draft and are not
    /// silently recalculated when household rules later change.
    pub fn create_weekly_plan_from_rules(
        &self,
        week_start: NaiveDate,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        if week_start.weekday() != Weekday::Mon {
            return Err(MealPlannerError::InvalidInput(
                "week_start must be a Monday".into(),
            ));
        }
        let resolved = self.resolve_meal_week(week_start)?;
        let mut assignments: Vec<MealAssignment> = Vec::new();
        for participation in resolved {
            let arrangement = match participation.status {
                ParticipationStatus::Unconfigured => continue,
                ParticipationStatus::Configured { arrangement, .. } => arrangement,
            };
            let assignment = assignments.iter_mut().find(|item| {
                item.date == participation.date
                    && item.meal_type == participation.meal_type
                    && item.arrangement == arrangement
            });
            let participant = PlanParticipant {
                member_id: participation.member.id,
                name: participation.member.name,
            };
            if let Some(assignment) = assignment {
                assignment.participants.push(participant);
            } else {
                let id = assignments.len() as i64 + 1;
                assignments.push(MealAssignment {
                    id,
                    date: participation.date,
                    meal_type: participation.meal_type,
                    participants: vec![participant],
                    arrangement,
                    recipe_ids: vec![],
                    school_menu_date: (arrangement == MealArrangement::School)
                        .then_some(participation.date),
                    notes: None,
                });
            }
        }
        self.create_weekly_plan(week_start, WeeklyPlanDraft { assignments })
    }

    pub fn create_weekly_plan(
        &self,
        week_start: NaiveDate,
        draft: WeeklyPlanDraft,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let draft = draft
            .validate(week_start)
            .map_err(MealPlannerError::InvalidInput)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        ensure_references(&connection, &draft)?;
        let revision: i64 = connection.query_row(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM weekly_plans WHERE family_id = 'legacy' AND week_start = ?1",
            [week_start.to_string()],
            |row| row.get(0),
        )?;
        let now = Utc::now().timestamp();
        let assignments = serde_json::to_string(&draft.assignments)?;
        connection.execute(
            "INSERT INTO weekly_plans(family_id, week_start, revision, status, assignments, created_at) VALUES ('legacy', ?1, ?2, 'draft', ?3, ?4)",
            params![week_start.to_string(), revision, assignments, now],
        )?;
        load_plan(&connection, connection.last_insert_rowid())?.ok_or(MealPlannerError::NotFound)
    }

    pub fn get_weekly_plan(
        &self,
        week_start: NaiveDate,
    ) -> Result<Option<WeeklyPlan>, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let id = connection
            .query_row(
                "SELECT id FROM weekly_plans WHERE family_id = 'legacy' AND week_start = ?1 ORDER BY revision DESC LIMIT 1",
                [week_start.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        match id {
            Some(id) => load_plan(&connection, id),
            None => Ok(None),
        }
    }

    pub fn list_weekly_plan_revisions(
        &self,
        week_start: NaiveDate,
    ) -> Result<Vec<WeeklyPlan>, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut statement = connection
            .prepare("SELECT id FROM weekly_plans WHERE family_id = 'legacy' AND week_start = ?1 ORDER BY revision DESC")?;
        let ids = statement
            .query_map([week_start.to_string()], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| load_plan(&connection, id)?.ok_or(MealPlannerError::NotFound))
            .collect()
    }

    /// Replace the latest draft. Confirmed plans are never updated in place.
    pub fn update_weekly_plan(
        &self,
        week_start: NaiveDate,
        draft: WeeklyPlanDraft,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let draft = draft
            .validate(week_start)
            .map_err(MealPlannerError::InvalidInput)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        ensure_references(&connection, &draft)?;
        let id = latest_plan_id(&connection, week_start)?.ok_or(MealPlannerError::NotFound)?;
        let status: String = connection.query_row(
            "SELECT status FROM weekly_plans WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        if status != "draft" {
            return Err(MealPlannerError::Conflict(
                "confirmed plans are immutable; create a revision first".into(),
            ));
        }
        let assignments = serde_json::to_string(&draft.assignments)?;
        connection.execute(
            "UPDATE weekly_plans SET assignments = ?1 WHERE id = ?2",
            params![assignments, id],
        )?;
        load_plan(&connection, id)?.ok_or(MealPlannerError::NotFound)
    }

    pub fn confirm_weekly_plan(
        &self,
        week_start: NaiveDate,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let id = latest_plan_id(&connection, week_start)?.ok_or(MealPlannerError::NotFound)?;
        let now = Utc::now().timestamp();
        let changed = connection.execute(
            "UPDATE weekly_plans SET status = 'confirmed', confirmed_at = ?1 WHERE id = ?2 AND status = 'draft'",
            params![now, id],
        )?;
        if changed == 0 {
            return Err(MealPlannerError::Conflict(
                "only a draft plan can be confirmed".into(),
            ));
        }
        load_plan(&connection, id)?.ok_or(MealPlannerError::NotFound)
    }

    /// Copy the latest confirmed snapshot into a new editable revision.
    pub fn revise_weekly_plan(
        &self,
        week_start: NaiveDate,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let id = latest_plan_id(&connection, week_start)?.ok_or(MealPlannerError::NotFound)?;
        let source = load_plan(&connection, id)?.ok_or(MealPlannerError::NotFound)?;
        if source.status != WeeklyPlanStatus::Confirmed {
            return Err(MealPlannerError::Conflict(
                "only a confirmed plan can be revised".into(),
            ));
        }
        let revision: i64 = connection.query_row(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM weekly_plans WHERE family_id = 'legacy' AND week_start = ?1",
            [week_start.to_string()],
            |row| row.get(0),
        )?;
        let now = Utc::now().timestamp();
        let assignments = serde_json::to_string(&source.assignments)?;
        connection.execute(
            "INSERT INTO weekly_plans(family_id, week_start, revision, status, assignments, created_at) VALUES ('legacy', ?1, ?2, 'draft', ?3, ?4)",
            params![week_start.to_string(), revision, assignments, now],
        )?;
        load_plan(&connection, connection.last_insert_rowid())?.ok_or(MealPlannerError::NotFound)
    }
}

fn ensure_family_references(
    connection: &rusqlite::Connection,
    family_id: &str,
    draft: &WeeklyPlanDraft,
) -> Result<(), MealPlannerError> {
    for assignment in &draft.assignments {
        for participant in &assignment.participants {
            let exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM family_members WHERE id=?1 AND family_id=?2 AND active=1)",
                params![participant.member_id, family_id], |row| row.get(0)
            )?;
            if !exists {
                return Err(MealPlannerError::NotFound);
            }
        }
        for recipe_id in &assignment.recipe_ids {
            let exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM recipes WHERE id=?1 AND (family_id IS NULL OR family_id=?2))",
                params![recipe_id, family_id], |row| row.get(0)
            )?;
            if !exists {
                return Err(MealPlannerError::NotFound);
            }
        }
    }
    Ok(())
}

fn latest_plan_id_for_family(
    connection: &rusqlite::Connection,
    family_id: &str,
    week_start: NaiveDate,
) -> Result<Option<i64>, MealPlannerError> {
    Ok(connection.query_row(
        "SELECT id FROM weekly_plans WHERE family_id=?1 AND week_start=?2 ORDER BY revision DESC LIMIT 1",
        params![family_id, week_start.to_string()], |row| row.get(0)
    ).optional()?)
}

fn ensure_references(
    connection: &rusqlite::Connection,
    draft: &WeeklyPlanDraft,
) -> Result<(), MealPlannerError> {
    for assignment in &draft.assignments {
        for participant in &assignment.participants {
            let exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM family_members WHERE id = ?1)",
                [participant.member_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(MealPlannerError::NotFound);
            }
        }
        for recipe_id in &assignment.recipe_ids {
            let exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM recipes WHERE id = ?1)",
                [recipe_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(MealPlannerError::NotFound);
            }
        }
    }
    Ok(())
}

fn latest_plan_id(
    connection: &rusqlite::Connection,
    week_start: NaiveDate,
) -> Result<Option<i64>, MealPlannerError> {
    Ok(connection
        .query_row(
            "SELECT id FROM weekly_plans WHERE family_id = 'legacy' AND week_start = ?1 ORDER BY revision DESC LIMIT 1",
            [week_start.to_string()],
            |row| row.get(0),
        )
        .optional()?)
}

pub(super) fn load_plan(
    connection: &rusqlite::Connection,
    id: i64,
) -> Result<Option<WeeklyPlan>, MealPlannerError> {
    connection.query_row(
        "SELECT id, week_start, revision, status, assignments, created_at, confirmed_at FROM weekly_plans WHERE id = ?1",
        [id],
        |row| {
            let week_start: String = row.get(1)?;
            let status: String = row.get(3)?;
            let assignments: String = row.get(4)?;
            let confirmed_at: Option<i64> = row.get(6)?;
            Ok((row.get::<_, i64>(0)?, week_start, row.get::<_, i64>(2)?, status, assignments, row.get::<_, i64>(5)?, confirmed_at))
        },
    ).optional()?.map(|(id, week_start, revision, status, assignments, created_at, confirmed_at)| {
        let week_start = NaiveDate::parse_from_str(&week_start, "%Y-%m-%d")
            .map_err(|error| MealPlannerError::InvalidInput(format!("invalid plan date in database: {error}")))?;
        let status = match status.as_str() {
            "draft" => WeeklyPlanStatus::Draft,
            "confirmed" => WeeklyPlanStatus::Confirmed,
            _ => return Err(MealPlannerError::InvalidInput("invalid plan status in database".into())),
        };
        Ok(WeeklyPlan {
            id,
            week_start,
            revision,
            status,
            assignments: serde_json::from_str(&assignments)?,
            created_at,
            confirmed_at,
        })
    }).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MealType, MealWeekday};

    fn monday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
    }

    #[test]
    fn creation_snapshots_participation_and_confirmation_is_immutable() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let member = store.create_family_member("Alex").unwrap();
        store
            .add_recurring_rule(
                member.id,
                MealWeekday::Monday,
                MealType::Lunch,
                MealArrangement::Home,
                None,
                None,
            )
            .unwrap();

        let draft = store.create_weekly_plan_from_rules(monday()).unwrap();
        assert_eq!(draft.status, WeeklyPlanStatus::Draft);
        assert_eq!(draft.assignments.len(), 1);
        assert_eq!(draft.assignments[0].participants[0].name, "Alex");

        let confirmed = store.confirm_weekly_plan(monday()).unwrap();
        assert_eq!(confirmed.status, WeeklyPlanStatus::Confirmed);
        assert!(confirmed.confirmed_at.is_some());

        let changed = WeeklyPlanDraft {
            assignments: vec![],
        };
        assert!(matches!(
            store.update_weekly_plan(monday(), changed),
            Err(MealPlannerError::Conflict(_))
        ));

        let revision = store.revise_weekly_plan(monday()).unwrap();
        assert_eq!(revision.revision, 2);
        assert_eq!(revision.status, WeeklyPlanStatus::Draft);
        assert_eq!(revision.assignments, confirmed.assignments);
    }

    #[test]
    fn revisions_are_ordered_and_latest_draft_can_be_updated() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let first = store
            .create_weekly_plan(
                monday(),
                WeeklyPlanDraft {
                    assignments: vec![],
                },
            )
            .unwrap();
        let updated = store
            .update_weekly_plan(
                monday(),
                WeeklyPlanDraft {
                    assignments: vec![],
                },
            )
            .unwrap();
        assert_eq!(first.id, updated.id);
        assert_eq!(store.list_weekly_plan_revisions(monday()).unwrap().len(), 1);
    }

    #[test]
    fn weekly_plans_and_revisions_are_isolated_by_family() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let week = monday();
        let first = store
            .create_weekly_plan_for_family(
                "family-a",
                week,
                WeeklyPlanDraft {
                    assignments: vec![],
                },
            )
            .unwrap();
        let second = store
            .create_weekly_plan_for_family(
                "family-b",
                week,
                WeeklyPlanDraft {
                    assignments: vec![],
                },
            )
            .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(second.revision, 1);
        assert_eq!(
            store
                .list_weekly_plan_revisions_for_family("family-a", week)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .list_weekly_plan_revisions_for_family("family-b", week)
                .unwrap()
                .len(),
            1
        );
        assert!(!store.plan_belongs_to_family(first.id, "family-b").unwrap());
    }
}
