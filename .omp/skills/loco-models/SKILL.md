---
name: loco-models
description: Loco.rs model generation, migrations, and SeaORM entity conventions
---

# Loco Model Generation

Covers model creation, migrations, entity generation, and custom ActiveModel logic in Loco.rs. Use this skill when creating new models, writing migrations, or extending SeaORM entities.

## Migration-First Workflow

1. Generate a migration (creates the table)
2. Apply the migration (`cargo loco db migrate`)
3. Generate entity code from the schema (`cargo loco db entities`)
4. Write custom ActiveModel logic in `src/models/<name>.rs`

## Generate a Model

```bash
cargo loco generate model <plural_name> field1:type1 field2:type2 ...
```

Creates the migration, applies it, and regenerates entities in one step.

### Field Syntax

| Suffix | Meaning |
|--------|---------|
| `!` | Required (`NOT NULL`) |
| `^` | Unique constraint |
| (none) | Nullable |

### Data Types

| Type | Example | DB Column |
|------|---------|-----------|
| `string` / `string!` | `name:string!` | `VARCHAR NOT NULL` |
| `text` / `text!` | `content:text!` | `TEXT NOT NULL` |
| `int` / `int!` | `count:int!` | `INTEGER NOT NULL` |
| `big_int` / `big_int!` | `big_count:big_int!` | `BIGINT NOT NULL` |
| `small_int` / `small_int!` | `status:small_int!` | `SMALLINT NOT NULL` |
| `float` / `float!` | `price:float!` | `FLOAT NOT NULL` |
| `double` / `double!` | `amount:double!` | `DOUBLE NOT NULL` |
| `decimal` / `decimal!` | `tax:decimal!` | `DECIMAL NOT NULL` |
| `bool` / `bool!` | `active:bool!` | `BOOLEAN NOT NULL` |
| `json` / `json!` | `metadata:json!` | `JSON NOT NULL` |
| `jsonb` / `jsonb!` | `config:jsonb!` | `JSONB NOT NULL` |
| `date_time` / `date_time!` | `started_at:date_time!` | `TIMESTAMP NOT NULL` |
| `tstz` / `tstz!` | `created_at:tstz!` | `TIMESTAMP WITH TIME ZONE NOT NULL` |
| `date` / `date!` | `event_date:date!` | `DATE NOT NULL` |
| `uuid` / `uuid!` | `pid:uuid!` | `UUID NOT NULL` |
| `blob` / `blob!` | `data:blob!` | `BLOB NOT NULL` |
| `<model>:references` | `user:references` | FK to `users.user_id` |

### Naming Conventions

- **Model names**: plural (e.g. `posts`, `movies`)
- **References**: singular (e.g. `user:references` references `users` table)
- **Column names**: `snake_case`
- **Table names**: plural, `snake_case`
- **Migration names**: `PascalCase`

## Generate a Migration Only

```bash
cargo loco generate migration <name> [field:type ...]
```

### Migration Naming Patterns

| Pattern | Example | Effect |
|---------|---------|--------|
| `Create___` | `CreatePosts` | Creates new table |
| `Add___To___` | `AddNameAndAgeToUsers` | Adds columns |
| `Remove___From___` | `RemoveNameAndAgeFromUsers` | Removes columns |
| `Add___RefTo___` | `AddUserRefToPosts` | Adds foreign key |
| `CreateJoinTable___And___` | `CreateJoinTableUsersAndGroups` | Creates join table |
| Other | `FixUsersTable` | Empty migration |

## Apply / Rollback Migrations

```bash
cargo loco db migrate    # apply pending migrations
cargo loco db down       # rollback last migration
cargo loco db down 2     # rollback last 2 migrations
```

## Regenerate Entities

```bash
cargo loco db entities
```

Regenerates `src/models/_entities/*.rs` from the current database schema. **Do not edit generated files directly.**

## Custom ActiveModel Logic

Custom behavior lives in `src/models/<name>.rs`:

```rust
impl super::_entities::posts::ActiveModel {
    pub async fn create_post(ctx: &AppContext, title: &str) -> Result<Self, ModelErr> {
        let mut post = ActiveModel {
            title: Set(Some(title.to_string())),
            ..Default::default()
        };
        post.insert(&ctx.db).await.map_err(ModelErr::from)
    }
}
```

## Testing

```rust
use loco_rs::testing::prelude::*;

#[tokio::test]
#[serial]
async fn can_find_by_id() {
    configure_insta!();
    let boot = boot_test::<App, Migrator>().await;
    seed::<App>(&boot.app_context).await.unwrap();

    let result = Model::find_by_id(&boot.app_context.db, 1).await;
    assert_debug_snapshot!(result);
}
```

## Key Constraints

1. **Do not edit `src/models/_entities/`** — files are auto-generated.
2. **Custom logic goes in `src/models/<name>.rs`** — extends the generated entity.
3. **Use `#[serial]` on tests** — avoids database contention.
4. **Run `cargo loco db entities` after migration changes** — keeps entities in sync.
