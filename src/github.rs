use crate::{
    config::GitHubConfig,
    errors::InternalError,
    model::{event::Event, events::Events},
};
use eyre::eyre;
use jsonwebtoken::EncodingKey;
use log::{info, warn};
use octocrab::{
    models::repos::Object, params::repos::Reference, pulls::PullRequestHandler, repos::RepoHandler,
    Octocrab, OctocrabBuilder,
};
use reqwest::{StatusCode, Url};
use std::fs;

/// The higher suffix number to add to a branch name.
const MAX_SUFFIX: u32 = 9;

async fn build_octocrab(config: &GitHubConfig) -> Result<Octocrab, InternalError> {
    let file_contents = fs::read(&config.private_key)?;
    let key = EncodingKey::from_rsa_pem(&file_contents)?;
    let octocrab = OctocrabBuilder::new()
        .app(config.app_id.into(), key)
        .build()?;

    // Get the installation for the repository we care about.
    let installation = octocrab
        .apps()
        .get_repository_installation(&config.owner, &config.repository)
        .await?;

    // Make an Octocrab for that installation.
    Ok(octocrab.installation(installation.id))
}

fn get_repo_pulls<'a>(
    octocrab: &'a Octocrab,
    config: &GitHubConfig,
) -> Result<(RepoHandler<'a>, PullRequestHandler<'a>), InternalError> {
    Ok((
        octocrab.repos(&config.owner, &config.repository),
        octocrab.pulls(&config.owner, &config.repository),
    ))
}

/// Creates a branch for the PR to add the given event, and returns its name.
async fn create_branch(
    repo: &RepoHandler<'_>,
    event: &Event,
    head_sha: &str,
) -> Result<String, InternalError> {
    // Create the branch, retrying with different suffixes if it already exists.
    let pr_branch_base = format!(
        "add-{}-{}-{}",
        to_safe_filename(&event.country),
        to_safe_filename(&event.city),
        to_safe_filename(&event.name),
    );

    let mut last_error = eyre!("Failed to create branch for event PR.");
    for suffix in 0..=MAX_SUFFIX {
        let branch_name = if suffix == 0 {
            pr_branch_base.clone()
        } else {
            format!("{}{}", pr_branch_base, suffix)
        };
        info!("Creating branch \"{}\"", branch_name);
        if let Err(e) = repo
            .create_ref(&Reference::Branch(branch_name.clone()), head_sha)
            .await
        {
            if matches!(&e, octocrab::Error::Http {source, .. }
        if source.status() == Some(StatusCode::UNPROCESSABLE_ENTITY))
            {
                // Probably the branch already exists, let the loop try a different suffix.
                last_error = e.into();
            } else {
                // Some other error, return immediately.
                return Err(e.into());
            }
        } else {
            return Ok(branch_name);
        }
    }

    warn!(
        "Failed to create PR branch {} after trying all suffixes: {}",
        pr_branch_base, last_error
    );
    Err(InternalError::Internal(last_error))
}

/// Creates a PR to add the given event to the given file.
///
/// Returns the URL of the new PR.
pub async fn add_event_to_file(
    event: Event,
    filename: String,
    config: &GitHubConfig,
) -> Result<Url, InternalError> {
    let octocrab = build_octocrab(config).await?;
    let (repo, pulls) = get_repo_pulls(&octocrab, config)?;

    let new_events = Events {
        events: vec![event.clone()],
    };
    let yaml = serde_yaml::to_string(&new_events)?;

    let head_sha = sha_for_branch(&repo, &config.main_branch).await?;
    let pr_branch = create_branch(&repo, &event, &head_sha).await?;

    // Create a commit to add or modify the file.
    let commit_message = format!("Add {} in {}", event.name, event.city);
    if let Ok(contents) = repo
        .get_content()
        .path(&filename)
        .r#ref(&pr_branch)
        .send()
        .await
    {
        // File already exists, add to it.
        let existing_file = &contents.items[0];
        let existing_content = existing_file.decoded_content().unwrap();

        // Append event to it.
        let formatted_event = yaml.trim_start_matches("---\nevents:\n");
        let new_content = format!("{}\n{}", existing_content, formatted_event);

        info!("Got existing file, sha {}", existing_file.sha);
        // Update the file
        let update = repo
            .update_file(&filename, &commit_message, new_content, &existing_file.sha)
            .branch(&pr_branch)
            .send()
            .await?;
        info!("Update: {:?}", update);
    } else {
        // File doesn't exist, create it.
        let content = yaml.replacen(
            "---",
            "# yaml-language-server: $schema=../../events_schema.json",
            1,
        );
        let create = repo
            .create_file(&filename, &commit_message, content)
            .branch(&pr_branch)
            .send()
            .await?;
        info!("Create: {:?}", create);
    }

    // Create PR for the branch.
    let pr = pulls
        .create(&commit_message, &pr_branch, &config.main_branch)
        .body("Added from web form.")
        .send()
        .await?;
    info!("Made PR {:?}", pr);
    let pr_url = pr
        .html_url
        .ok_or_else(|| InternalError::Internal(eyre!("PR missing html_url")))?;
    Ok(pr_url)
}

/// Returns the SHA for the current head of the given branch.
async fn sha_for_branch(
    repo: &RepoHandler<'_>,
    branch_name: &str,
) -> Result<String, InternalError> {
    let head = repo
        .get_ref(&Reference::Branch(branch_name.to_owned()))
        .await?;
    if let Object::Commit { sha, .. } = head.object {
        Ok(sha)
    } else {
        Err(InternalError::Internal(eyre!(
            "Ref {} was not a commit.",
            branch_name
        )))
    }
}

/// Converts the given string to a suitable filename by converting it to lowercase, replacing spaces
/// with underscores, and removing special characters.
///
/// The returned string will only contain ASCII alphanumeric characters, underscores and hyphens.
pub fn to_safe_filename(s: &str) -> String {
    let mut filename = s.to_lowercase().replace(' ', "_");
    filename.retain(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    filename.truncate(30);
    filename
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_filenames() {
        assert_eq!(to_safe_filename("Southend-on-Sea"), "southend-on-sea");
        assert_eq!(
            to_safe_filename("weird'\"@\\/ characters"),
            "weird_characters"
        )
    }
}
