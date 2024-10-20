#!/bin/bash

PASSWORD=`head -c1000 /dev/urandom | tr -dc [:alpha:][:digit:] | head -c 16; echo ;`
JWT_SECRET=`head -c1000 /dev/urandom | tr -dc [:alpha:][:digit:] | head -c 32; echo ;`
SECRET_KEY=`head -c1000 /dev/urandom | tr -dc [:alpha:][:digit:] | head -c 32; echo ;`
DB=weather_data_cache

sudo apt-get install -y postgresql

sudo -u postgres createuser -E -e $USER
sudo -u postgres psql -c "CREATE ROLE $USER PASSWORD '$PASSWORD' NOSUPERUSER NOCREATEDB NOCREATEROLE INHERIT LOGIN;"
sudo -u postgres psql -c "ALTER ROLE $USER PASSWORD '$PASSWORD' NOSUPERUSER NOCREATEDB NOCREATEROLE INHERIT LOGIN;"
sudo -u postgres createdb $DB
sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE $DB TO $USER;"
sudo -u postgres psql $DB -c "GRANT ALL ON SCHEMA public TO $USER;"

mkdir -p ${HOME}/.config/aws_app_rust
cat > ${HOME}/.config/sync_app_rust/config.env <<EOL
DATABASE_URL=postgresql://$USER:$PASSWORD@localhost:5432/$DB
MY_OWNER_ID=8675309
DEFAULT_SECURITY_GROUP=sg-0
SPOT_SECURITY_GROUP=sg-0
DEFAULT_KEY_NAME=default-key
SCRIPT_DIRECTORY=~/
DOMAIN=localhost
JWT_SECRET=$JWT_SECRET
SECRET_KEY=$SECRET_KEY
EOL
