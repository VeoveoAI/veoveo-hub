//! The installation's own skills, `computers.skills`, as the Hub serves them to a
//! Computer's own clients at `skill://veoveo/{name}/{path}`: each skill with its verbatim
//! frontmatter and every file with its SHA-256 and size, computed once when the
//! configuration loads, by core's rules for skills.
use std::collections::BTreeMap;

use rmcp::model::{
    ReadResourceResult, ResourceContents, Skill, SkillFrontmatter, SkillResource, SkillResources,
};
use veoveo_hub_sdk::skills::{HUB_AUTHORITY, SCHEME, SKILL_MD, SkillUri, media_type};

/// The configuration's skills: each skill name to its files, each path relative to the
/// skill's directory to the file's UTF-8 text.
pub type SkillFiles = BTreeMap<String, BTreeMap<String, String>>;

/// The Hub's own skills, in name order, and their files by address.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HubSkills {
    skills: Vec<Skill>,
    files: BTreeMap<String, String>,
}

/// Why the configuration's skills cannot be served.
#[derive(Debug, thiserror::Error)]
pub enum HubSkillsError {
    #[error(transparent)]
    Skill(#[from] veoveo_skills::SkillsError),
    #[error("skill {skill}: {reason}")]
    Invalid { skill: String, reason: String },
}

impl HubSkills {
    /// Every skill of `config`, each read with core's `veoveo_skills::load::skill` and
    /// projected onto the MCP Skills extension under the Hub's own authority.
    pub fn load(config: &SkillFiles) -> Result<Self, HubSkillsError> {
        let mut skills = Vec::with_capacity(config.len());
        let mut files = BTreeMap::new();
        for (name, texts) in config {
            let loaded = veoveo_skills::load::skill(
                name,
                texts
                    .iter()
                    .map(|(path, text)| (path.clone(), text.as_bytes().to_vec())),
            )?;
            let invalid = |reason: String| HubSkillsError::Invalid {
                skill: name.clone(),
                reason,
            };
            let frontmatter: SkillFrontmatter =
                serde_json::from_value(serde_json::Value::Object(loaded.frontmatter))
                    .map_err(|error| invalid(format!("frontmatter: {error}")))?;
            let mut resources = Vec::with_capacity(loaded.files.len());
            for file in &loaded.files {
                let uri = address(name, &file.path)
                    .map_err(|error| invalid(format!("{}: {error}", file.path)))?;
                resources.push(SkillResource::new(
                    uri.as_str(),
                    file.digest.to_string(),
                    file.size,
                ));
                files.insert(uri.as_str().to_owned(), texts[&file.path].clone());
            }
            let skill_md = address(name, SKILL_MD).map_err(|error| invalid(error.to_string()))?;
            let skill = Skill::new(
                skill_md.as_str(),
                frontmatter,
                SkillResources::Files(resources),
            );
            skill
                .validate()
                .map_err(|error| invalid(error.to_string()))?;
            skills.push(skill);
        }
        Ok(Self { skills, files })
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Every skill's entry, in name order.
    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// The skill whose `SKILL.md` is at `uri`.
    pub fn get(&self, uri: &str) -> Option<&Skill> {
        self.skills.iter().find(|skill| skill.uri == uri)
    }

    /// The file at `uri` as a read answers it: its text, with a media type from its
    /// extension.
    pub fn read(&self, uri: &str) -> Option<ReadResourceResult> {
        let text = self.files.get(uri)?;
        let path = SkillUri::parse(uri).ok()?;
        Some(ReadResourceResult::new(vec![
            ResourceContents::text(text, uri).with_mime_type(media_type(path.path())),
        ]))
    }
}

/// The Hub's own address of `path` in skill `name`.
fn address(name: &str, path: &str) -> Result<SkillUri, veoveo_hub_sdk::skills::SkillUriError> {
    SkillUri::parse(&format!("{SCHEME}://{HUB_AUTHORITY}/{name}/{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "---\nname: home\ndescription: Your Computer's home and what it keeps.\nmetadata:\n  audience: computers\n---\n# Home\n";

    fn config(entries: &[(&str, &[(&str, &str)])]) -> SkillFiles {
        entries
            .iter()
            .map(|(name, files)| {
                (
                    (*name).to_owned(),
                    files
                        .iter()
                        .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn the_configurations_skills_are_served_under_the_hubs_authority() {
        let skills = HubSkills::load(&config(&[(
            "home",
            &[("SKILL.md", HOME), ("references/lore.md", "abc")],
        )]))
        .unwrap();
        let [skill] = skills.skills() else {
            panic!("one skill")
        };
        assert_eq!(skill.uri, "skill://veoveo/home/SKILL.md");
        assert_eq!(skill.frontmatter.extra["metadata"]["audience"], "computers");
        let SkillResources::Files(files) = &skill.resources else {
            panic!("a full manifest")
        };
        assert_eq!(
            files
                .iter()
                .map(|file| (file.uri.as_str(), file.digest.as_str(), file.size))
                .collect::<Vec<_>>(),
            [
                (
                    "skill://veoveo/home/SKILL.md",
                    rmcp::model::sha256_digest(HOME.as_bytes()).as_str(),
                    HOME.len() as u64
                ),
                (
                    "skill://veoveo/home/references/lore.md",
                    // FIPS 180-2's SHA-256 test vector for "abc".
                    "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                    3
                ),
            ]
        );
        assert_eq!(skills.get("skill://veoveo/home/SKILL.md"), Some(skill));
        let read = serde_json::to_value(
            skills
                .read("skill://veoveo/home/references/lore.md")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(read["contents"][0]["text"], "abc");
        assert_eq!(read["contents"][0]["mimeType"], "text/markdown");
        assert!(skills.read("skill://veoveo/home/missing.md").is_none());
        assert!(HubSkills::load(&SkillFiles::new()).unwrap().is_empty());
    }

    #[test]
    fn a_skill_a_host_would_refuse_is_refused_when_the_configuration_loads() {
        for invalid in [
            config(&[("Home", &[("SKILL.md", HOME)])]),
            config(&[("home", &[("notes.md", "x")])]),
            config(&[("home", &[("SKILL.md", HOME), ("../escape.md", "x")])]),
            config(&[("home", &[("SKILL.md", HOME), ("/etc/passwd", "x")])]),
            config(&[("other", &[("SKILL.md", HOME)])]),
            config(&[("home", &[("SKILL.md", HOME), ("a b.md", "x")])]),
        ] {
            assert!(HubSkills::load(&invalid).is_err(), "{invalid:?}");
        }
    }
}
