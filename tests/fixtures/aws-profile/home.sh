#!/usr/bin/env bash
# Makes a throwaway home for an AWS profile e2e run (ADR-2610091530) and prints the variables
# Wardian runs with there, one per line.
#
# usage: home.sh process|cli DIR
#   process  ~/.aws/config's profile "e2e" uses credential_process (tests/fixtures/aws-profile/creds);
#            WARDIAN_AWS_CLI=none, so Wardian runs it itself even where an AWS CLI is installed.
#   cli      profile "e2e" is an SSO profile only the AWS CLI can sign in to; a fake `aws`
#            (tests/fixtures/aws-profile/aws) comes first on PATH.
set -euo pipefail
MODE=$1 DIR=$2
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$DIR/home/.aws" "$DIR/bin"
chmod +x "$HERE/creds" "$HERE/aws"
if [ "$MODE" = process ]; then
  cat >"$DIR/home/.aws/config" <<EOF
# A profile whose keys come from a program, as for a password manager or a company tool.
[default]
region = eu-west-3

[profile e2e]
region = us-east-1
credential_process = "$HERE/creds" --profile 'e2e run'
EOF
  echo "WARDIAN_AWS_CLI=none"
else
  cat >"$DIR/home/.aws/config" <<EOF
[profile e2e]
sso_session = corp
sso_account_id = 111122223333
sso_role_name = Developer
region = us-east-1

[profile e2e-expired]
sso_session = corp
sso_account_id = 111122223333
sso_role_name = Developer
region = us-east-1

[sso-session corp]
sso_start_url = https://corp.awsapps.com/start
sso_region = us-east-1
EOF
  ln -sf "$HERE/aws" "$DIR/bin/aws"
  echo "PATH=$DIR/bin:$PATH"
fi
echo "HOME=$DIR/home"
echo "FAKE_AWS_LOG=$DIR/aws.log"
