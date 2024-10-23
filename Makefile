.PHONY: check format lint docs build stop run logs run-logs restart restart-logs services prepare rabbitmq r symlink migration migrate revert remigrate

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

stop-app:
	@-docker compose down scheduler
	@-docker compose down worker

rabbitmq:
	@docker compose up -d rabbitmq

r: rabbitmq

services:
	@docker compose up -d rabbitmq postgres pgadmin

run:
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

# 
symlink:
	ln -s "$(PWD)" /tmp/bms

# Usage: make migration name=migration_name
# For some reason add with -r and --source doesnt produce up and down files in the correct directory, so we move them manually
migration:
	$(call check_name_defined)
	@sqlx migrate add -r $(name)
	mv migrations/* $(migration_source)/

migrate:
	@cargo sqlx migrate run --source $(migration_source)

revert:
	@cargo sqlx migrate revert --source $(migration_source)

remigrate: revert migrate