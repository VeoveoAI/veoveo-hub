//! The people who signed in, by tenant (`crate::person`).
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use veoveo_types::{PrincipalId, TenantId};

use crate::{
    contract::PrincipalDisplayName,
    person::{Email, Person},
};

/// How many times a sign-in records its person while concurrent sign-ins take its email.
const ATTEMPTS: usize = 3;

impl crate::store::HubStore {
    /// Record `person` as their sign-in names them, taking their email from any other
    /// person of the tenant.
    pub async fn record_person(&self, person: &Person, now: DateTime<Utc>) -> Result<()> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.try_record_person(person, now).await {
                // A concurrent sign-in recorded the same email first; taking it over
                // again sees that person's committed row.
                Err(sqlx::Error::Database(error))
                    if error.is_unique_violation() && attempt < ATTEMPTS => {}
                result => return result.context("record the person who signed in"),
            }
        }
    }

    async fn try_record_person(&self, person: &Person, now: DateTime<Utc>) -> sqlx::Result<()> {
        let mut transaction = self.pool().begin().await?;
        if let Some(email) = &person.email {
            sqlx::query_file!(
                "src/store/person/release.sql",
                person.tenant.as_str(),
                email.as_str(),
                person.principal.as_str(),
                now
            )
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query_file!(
            "src/store/person/record.sql",
            person.tenant.as_str(),
            person.principal.as_str(),
            person.name.as_str(),
            person.email.as_ref().map(Email::as_str),
            now
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await
    }

    /// The person of the tenant whose email is `email`, if one who signed in has it.
    pub async fn person_by_email(
        &self,
        tenant: &TenantId,
        email: &Email,
    ) -> Result<Option<Person>> {
        let Some(row) = sqlx::query_file!(
            "src/store/person/by_email.sql",
            tenant.as_str(),
            email.as_str()
        )
        .fetch_optional(self.pool())
        .await
        .context("read the person of an email")?
        else {
            return Ok(None);
        };
        Ok(Some(Person {
            tenant: tenant.clone(),
            principal: PrincipalId::parse(row.principal)
                .context("stored person names no principal")?,
            name: PrincipalDisplayName::new(row.display_name)
                .context("stored person has no display name")?,
            email: Some(email.clone()),
        }))
    }

    /// The people of the tenant among `principals` who signed in; one who never did is
    /// missing from the answer.
    pub async fn people(
        &self,
        tenant: &TenantId,
        principals: &[PrincipalId],
    ) -> Result<Vec<Person>> {
        let principals = principals
            .iter()
            .map(|principal| principal.as_str().to_owned())
            .collect::<Vec<_>>();
        sqlx::query_file!(
            "src/store/person/by_principals.sql",
            tenant.as_str(),
            &principals
        )
        .fetch_all(self.pool())
        .await
        .context("read the people of principals")?
        .into_iter()
        .map(|row| {
            Ok(Person {
                tenant: tenant.clone(),
                principal: PrincipalId::parse(row.principal)
                    .context("stored person names no principal")?,
                name: PrincipalDisplayName::new(row.display_name)
                    .context("stored person has no display name")?,
                email: row
                    .email
                    .map(|email| Email::parse(&email))
                    .transpose()
                    .context("stored person has an invalid email")?,
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::HubStore;

    fn person(tenant: &str, principal: &str, name: &str, email: Option<&str>) -> Person {
        Person {
            tenant: TenantId::parse(tenant).unwrap(),
            principal: PrincipalId::parse(principal).unwrap(),
            name: PrincipalDisplayName::new(name).unwrap(),
            email: email.map(|email| Email::parse(email).unwrap()),
        }
    }

    /// Each sign-in replaces its person's row, and an email belongs to the person of the
    /// tenant who signed in with it last.
    #[tokio::test]
    async fn a_sign_in_records_its_person_and_takes_over_its_email() {
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            let db = crate::test_store::TestDb::new().await;
            let store = HubStore::new(db.a.clone());
            let tenant = TenantId::parse("tenant-a").unwrap();
            let email = |value: &str| Email::parse(value).unwrap();
            let now = Utc::now();
            let alex = person("tenant-a", "idp#alex", "Alex", Some("alex@example.com"));
            store.record_person(&alex, now).await.unwrap();
            assert_eq!(
                store
                    .person_by_email(&tenant, &email("alex@example.com"))
                    .await
                    .unwrap(),
                Some(alex.clone())
            );
            // Another tenant's person with the same email is another person.
            let elsewhere = person("tenant-b", "idp#other", "Other", Some("alex@example.com"));
            store.record_person(&elsewhere, now).await.unwrap();
            assert_eq!(
                store
                    .person_by_email(&tenant, &email("alex@example.com"))
                    .await
                    .unwrap(),
                Some(alex.clone())
            );

            // A later sign-in replaces the name and the email.
            let renamed = person("tenant-a", "idp#alex", "Alex R", Some("alex@new.example"));
            store.record_person(&renamed, now).await.unwrap();
            assert_eq!(
                store
                    .person_by_email(&tenant, &email("alex@example.com"))
                    .await
                    .unwrap(),
                None
            );
            assert_eq!(
                store
                    .person_by_email(&tenant, &email("alex@new.example"))
                    .await
                    .unwrap(),
                Some(renamed.clone())
            );

            // Another person signing in with that email takes it over.
            let sam = person("tenant-a", "idp#sam", "Sam", Some("alex@new.example"));
            store.record_person(&sam, now).await.unwrap();
            assert_eq!(
                store
                    .person_by_email(&tenant, &email("alex@new.example"))
                    .await
                    .unwrap(),
                Some(sam.clone())
            );

            // A sign-in without an address records the person without one.
            let kim = person("tenant-a", "idp#kim", "Kim", None);
            store.record_person(&kim, now).await.unwrap();
            let principals = ["idp#alex", "idp#sam", "idp#kim", "idp#never"]
                .map(|principal| PrincipalId::parse(principal).unwrap());
            let mut people = store.people(&tenant, &principals).await.unwrap();
            people.sort_by(|a, b| a.principal.cmp(&b.principal));
            let mut released = renamed.clone();
            released.email = None;
            assert_eq!(
                people,
                [released, kim, sam],
                "one who never signed in is missing"
            );
        })
        .await
        .expect("the person test exceeded its bound");
    }
}
