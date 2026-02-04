# Makefile
include .env
export

.PHONY: check format lint docs build stop run logs run-logs restart restart-logs services prepare rabbitmq r rm-symlink symlink migration migrate-up migrate-down migrate-status remigrate init-dev

migration_source = $(PWD)/scheduler/src/migrations

define check_name_defined
	@if [ -z "$(name)" ]; then \
		echo "Error: name variable is not defined."; \
		echo "Usage: make $(MAKECMDGOALS) name=migration_name"; \
		exit 1; \
	fi
endef

check: 
	@-cargo fmt -- --check
	@cargo clippy

format:
	@cargo fmt

lint:
	@cargo clippy

docs:
	@cargo doc --no-deps --document-private-items 

build:
	@docker compose build 

stop:
	@docker compose down

clear:
	@docker compose down -v

stop-app:
	@-docker compose down scheduler
	@-docker compose down worker

rabbitmq:
	@docker compose up -d rabbitmq

r: rabbitmq

services:
	@docker compose up -d rabbitmq postgres pgadmin

run: rm-symlink symlink
	@docker compose up -d

logs:
	@docker compose logs -f -n 100

run-logs: run logs
restart: stop-app build run-logs
restart-logs: stop 

prepare:
	@cargo sqlx prepare --workspace
	@cargo fmt
	@cargo clippy

rm-symlink:
	@-rm -R /tmp/bms

# required for the scheduler to access files from host to spawn new docker containers seamlessly on the host
symlink:
	@if [ ! -L /tmp/bms ] || [ "$$(readlink /tmp/bms)" != "$(PWD)" ]; then \
		rm -rf /tmp/bms; \
		ln -s "$(PWD)" /tmp/bms; \
	fi

# Usage: make migration name=migration_name
# For some reason add with -r and --source doesnt produce up and down files in the correct directory, so we move them manually
migration:
	$(call check_name_defined)
	@sqlx migrate add -r $(name)
	mv migrations/* $(migration_source)/

migrate-up:
	@cargo sqlx migrate run --source $(migration_source)

migrate-down:
	@cargo sqlx migrate revert --source $(migration_source)

migrate-status:
	@cargo sqlx migrate info --source $(migration_source)

remigrate: migrate-down migrate-up

init-dev: services symlink
	@until pg_isready -h localhost -p 5442 -U pguser > /dev/null 2>&1; do sleep 1; done
	@$(MAKE) migrate-up
	@$(PWD)/scripts/init_local_services.sh