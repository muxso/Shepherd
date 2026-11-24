#!/usr/bin/env bash
set -euo pipefail

: "${SONAR_HOST_URL:?Set the SonarQube server URL}"
: "${SONAR_PROJECT_KEY:?Set the SonarQube project key}"
: "${SONAR_TOKEN:?Inject a project analysis token through CI secrets}"
: "${CODEUP_DEFAULT_BRANCH:?Set the default branch configured in Codeup}"
: "${CODEUP_SCAN_REVISION:?Set the commit SHA from the Codeup push event}"

if [[ ! $CODEUP_SCAN_REVISION =~ ^[0-9a-f]{40}$ ]]; then
    printf 'Expected a full Codeup commit SHA\n' >&2
    exit 2
fi

scan_branch=${CODEUP_SCAN_BRANCH:-$CODEUP_DEFAULT_BRANCH}
git check-ref-format "refs/heads/$scan_branch"
if [[ $(git rev-parse HEAD) != "$CODEUP_SCAN_REVISION" ]]; then
    printf 'CI checkout does not match the Codeup push event\n' >&2
    exit 2
fi
git fetch --quiet --no-tags origin "refs/heads/$scan_branch"
if [[ $(git rev-parse FETCH_HEAD) != "$CODEUP_SCAN_REVISION" ]]; then
    printf 'Skipping a commit that is no longer the tip of %s\n' "$scan_branch"
    exit 0
fi

commit_author=$(git show -s --format=%an "$CODEUP_SCAN_REVISION")
sonar_args=(
    "-Dsonar.projectKey=$SONAR_PROJECT_KEY"
    "-Dsonar.scm.revision=$CODEUP_SCAN_REVISION"
    "-Dsonar.analysis.commitAuthor=$commit_author"
)

if [[ -n ${CODEUP_SCAN_BRANCH:-} ]]; then
    sonar_args+=("-Dsonar.branch.name=$scan_branch")
fi

case "${SONAR_SCANNER_MODE:-}" in
    maven)
        env -u SONAR_TOKEN mvn -B -ntp clean install
        mvn -B -ntp org.sonarsource.scanner.maven:sonar-maven-plugin:5.5.0.6356:sonar "${sonar_args[@]}"
        ;;
    cli)
        sonar-scanner "${sonar_args[@]}"
        ;;
    *)
        printf 'Set SONAR_SCANNER_MODE to maven or cli\n' >&2
        exit 2
        ;;
esac
