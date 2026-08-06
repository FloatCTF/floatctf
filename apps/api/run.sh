#!/bin/bash
sea-orm-cli generate entity -o src/entity --with-serde both --enum-extra-attributes 'serde(rename_all = "snake_case")'
RUSTFLAGS="-A warnings" cargo watch --watch src/ --watch fcmc/src --ignore src/sql/ -x run
