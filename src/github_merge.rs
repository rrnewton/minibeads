//! Three-way reconciliation of the fields shared with GitHub Issues.

use crate::github_ancestor::{
    AncestorFields, CommonIssueBody, CommonIssueFields, CommonIssueTitle,
};
use crate::prose_merge::{
    merge_prose, CompetingEditReason, ProseMergeLimits, ProseMergeResult, StructuredOverlapReason,
    WorkExhaustionReason,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProseMergeFailureKind {
    CompetingEdit(CompetingEditReason),
    UnsupportedStructuredOverlap(StructuredOverlapReason),
    WorkExhausted(WorkExhaustionReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GithubBodyMergeInputs {
    ancestor: CommonIssueBody,
    local: CommonIssueBody,
    remote: CommonIssueBody,
}

impl GithubBodyMergeInputs {
    fn new(ancestor: &CommonIssueBody, local: &CommonIssueBody, remote: &CommonIssueBody) -> Self {
        Self {
            ancestor: CommonIssueBody::new(ancestor.as_str()),
            local: CommonIssueBody::new(local.as_str()),
            remote: CommonIssueBody::new(remote.as_str()),
        }
    }

    #[cfg(test)]
    pub(crate) fn ancestor(&self) -> &CommonIssueBody {
        &self.ancestor
    }

    #[cfg(test)]
    pub(crate) fn local(&self) -> &CommonIssueBody {
        &self.local
    }

    #[cfg(test)]
    pub(crate) fn remote(&self) -> &CommonIssueBody {
        &self.remote
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GithubBodyMergeFailure {
    kind: ProseMergeFailureKind,
    inputs: GithubBodyMergeInputs,
}

impl GithubBodyMergeFailure {
    fn new(
        kind: ProseMergeFailureKind,
        ancestor: &CommonIssueBody,
        local: &CommonIssueBody,
        remote: &CommonIssueBody,
    ) -> Self {
        Self {
            kind,
            inputs: GithubBodyMergeInputs::new(ancestor, local, remote),
        }
    }

    pub(crate) const fn kind(&self) -> ProseMergeFailureKind {
        self.kind
    }

    #[cfg(test)]
    pub(crate) const fn inputs(&self) -> &GithubBodyMergeInputs {
        &self.inputs
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GithubFieldMergeFailure {
    Title,
    Body(GithubBodyMergeFailure),
    Status,
}

impl std::fmt::Display for GithubFieldMergeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Title => formatter.write_str("title"),
            Self::Status => formatter.write_str("open/closed state"),
            Self::Body(failure) => match failure.kind() {
                ProseMergeFailureKind::CompetingEdit(reason) => {
                    write!(formatter, "description competing edit ({reason:?})")
                }
                ProseMergeFailureKind::UnsupportedStructuredOverlap(reason) => {
                    write!(formatter, "description structured overlap ({reason:?})")
                }
                ProseMergeFailureKind::WorkExhausted(reason) => {
                    write!(formatter, "description work limit ({reason:?})")
                }
            },
        }
    }
}

pub(crate) fn merge_issue_fields(
    ancestor: &AncestorFields,
    local: &CommonIssueFields,
    remote: &CommonIssueFields,
) -> Result<CommonIssueFields, GithubFieldMergeFailure> {
    let title = merge_text(
        ancestor.title().as_str(),
        local.title().as_str(),
        remote.title().as_str(),
    )
    .ok_or(GithubFieldMergeFailure::Title)?;
    let status = merge_scalar(ancestor.status(), local.status(), remote.status())
        .ok_or(GithubFieldMergeFailure::Status)?;
    let body = match merge_prose(
        ancestor.body().as_str(),
        local.body().as_str(),
        remote.body().as_str(),
        ProseMergeLimits::default(),
    ) {
        ProseMergeResult::Merged(body) => body.into_text().into_owned(),
        ProseMergeResult::CompetingEdit(failure) => {
            return Err(GithubFieldMergeFailure::Body(GithubBodyMergeFailure::new(
                ProseMergeFailureKind::CompetingEdit(failure.reason()),
                ancestor.body(),
                local.body(),
                remote.body(),
            )));
        }
        ProseMergeResult::UnsupportedStructuredOverlap(failure) => {
            return Err(GithubFieldMergeFailure::Body(GithubBodyMergeFailure::new(
                ProseMergeFailureKind::UnsupportedStructuredOverlap(failure.reason()),
                ancestor.body(),
                local.body(),
                remote.body(),
            )));
        }
        ProseMergeResult::WorkExhausted(failure) => {
            return Err(GithubFieldMergeFailure::Body(GithubBodyMergeFailure::new(
                ProseMergeFailureKind::WorkExhausted(failure.reason()),
                ancestor.body(),
                local.body(),
                remote.body(),
            )));
        }
    };
    Ok(CommonIssueFields::new(
        CommonIssueTitle::new(title),
        CommonIssueBody::new(body),
        status,
    ))
}

fn merge_text<'a>(ancestor: &'a str, local: &'a str, remote: &'a str) -> Option<&'a str> {
    if local == remote || remote == ancestor {
        Some(local)
    } else if local == ancestor {
        Some(remote)
    } else {
        None
    }
}

fn merge_scalar<Value: Copy + Eq>(ancestor: Value, local: Value, remote: Value) -> Option<Value> {
    if local == remote || remote == ancestor {
        Some(local)
    } else if local == ancestor {
        Some(remote)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github_ancestor::CommonIssueStatus;

    fn fields(title: &str, body: &str, status: CommonIssueStatus) -> CommonIssueFields {
        CommonIssueFields::new(
            CommonIssueTitle::new(title),
            CommonIssueBody::new(body),
            status,
        )
    }

    fn body_failure(
        result: Result<CommonIssueFields, GithubFieldMergeFailure>,
    ) -> GithubBodyMergeFailure {
        match result {
            Err(GithubFieldMergeFailure::Body(failure)) => failure,
            unexpected => panic!("expected body merge failure, got {unexpected:?}"),
        }
    }

    #[test]
    fn independent_fields_merge() {
        let ancestor = fields("Old", "Base.", CommonIssueStatus::Open);
        let local = fields("Local", "Base.", CommonIssueStatus::Open);
        let remote = fields("Old", "Remote.", CommonIssueStatus::Closed);
        let merged = merge_issue_fields(&ancestor, &local, &remote).unwrap();
        assert_eq!(merged.title().as_str(), "Local");
        assert_eq!(merged.body().as_str(), "Remote.");
        assert_eq!(merged.status(), CommonIssueStatus::Closed);
    }

    #[test]
    fn same_scalar_field_changes_conflict() {
        let ancestor = fields("Old", "Base.", CommonIssueStatus::Open);
        let local = fields("Local", "Base.", CommonIssueStatus::Open);
        let remote = fields("Remote", "Base.", CommonIssueStatus::Open);
        assert_eq!(
            merge_issue_fields(&ancestor, &local, &remote),
            Err(GithubFieldMergeFailure::Title)
        );
    }

    #[test]
    fn prose_failure_categories_are_preserved() {
        let ancestor = fields("Title", "foo", CommonIssueStatus::Open);
        let local = fields("Title", "bar", CommonIssueStatus::Open);
        let remote = fields("Title", "baz", CommonIssueStatus::Open);
        let failure = body_failure(merge_issue_fields(&ancestor, &local, &remote));
        assert_eq!(
            failure.kind(),
            ProseMergeFailureKind::CompetingEdit(CompetingEditReason::OverlappingEdits),
        );
        assert_eq!(failure.inputs().ancestor().as_str(), "foo");
        assert_eq!(failure.inputs().local().as_str(), "bar");
        assert_eq!(failure.inputs().remote().as_str(), "baz");

        let ancestor = fields("Title", "````\nbase\n````", CommonIssueStatus::Open);
        let local = fields("Title", "````\nlocal\n````", CommonIssueStatus::Open);
        let remote = fields("Title", "````\nremote\n````", CommonIssueStatus::Open);
        let failure = body_failure(merge_issue_fields(&ancestor, &local, &remote));
        assert_eq!(
            failure.kind(),
            ProseMergeFailureKind::UnsupportedStructuredOverlap(
                StructuredOverlapReason::MarkdownSource
            ),
        );
        assert_eq!(failure.inputs().ancestor().as_str(), "````\nbase\n````");
        assert_eq!(failure.inputs().local().as_str(), "````\nlocal\n````");
        assert_eq!(failure.inputs().remote().as_str(), "````\nremote\n````");
    }
}
