# Loco Model Generation

## Overview

Loco uses a **migration-first** workflow with SeaORM. The lifecycle is:

1. Generate a migration (creates the table)
2. Apply the migration (`cargo loco db migrate`)
3. Generate entity code from the schema (`cargo loco db entities`)
4. Write custom ActiveModel logic in `src/models/<name>.rs`

## Generate a Model

```bash
cargo loco generate model <plural_name> field1:type1 field2:type2 ...
```

This creates the migration, applies it, and regenerates entities in one step.

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
| `uuid` / `uuid!` / `uuid^` | `pid:uuid!` | `UUID NOT NULL` |
| `blob` / `blob!` | `data:blob!` | `BLOB NOT NULL` |
| `money` / `money!` | `cost:money!` | `MONEY NOT NULL` |
| `<model>:references` | `user:references` | FK to `users.user_id` |
| `<model>:references:<col>` | `user:references:authored_by` | FK to `users` as `authored_by` |

### Naming Conventions

- **Model names**: plural, e.g. `posts`, `movies`
- **References**: singular, e.g. `user:references` (references `users` table)
- **Column names**: `snake_case`
- **Table names**: plural, `snake_case` (auto-generated from model name)
- **Migration names**: `PascalCase`, e.g. `CreatePosts`

## Generate a Migration Only

```bash
cargo loco generate migration <name> [field:type ...]
```

Migration naming determines type:

| Pattern | Example | Effect |
|---------|---------|--------|
| `Create___` | `CreatePosts` | Creates new table |
| `Add___To___` | `AddNameAndAgeToUsers` | Adds columns to existing table |
| `Remove___From___` | `RemoveNameAndAgeFromUsers` | Removes columns |
| `Add___RefTo___` | `AddUserRefToPosts` | Adds foreign key reference |
| `CreateJoinTable___And___` | `CreateJoinTableUsersAndGroups` | Creates join table |
| Other | `FixUsersTable` | Empty migration |

## Apply / Rollback Migrations

```bash
cargo loco db migrate    # apply pending migrations
cargo loco db down       # rollback last migration
cargo loco db down 2     # rollback last 2 migrations
```

## Regenerate Entities

After manual migration edits or schema changes:

```bash
cargo loco db entities
```

This regenerates `src/models/_entities/*.rs` from the current database schema.

## Custom ActiveModel Logic

Custom behavior lives in `src/models/<name>.rs`, extending the generated entity:

```rust
// src/models/posts.rs
impl super::_entities::posts::ActiveModel {
    pub async fn create_post(ctx: &AppContext, title: &str, content: &str) -> Result<Self, ModelErr> {
        let mut post = ActiveModel {
            title: Set(Some(title.to_string())),
            content: Set(Some(content.to_string())),
            ..Default::default()
        };
        post.insert(&ctx.db).await.map_err(ModelErr::from)
    }
}
```

## Migrations DSL

For manual migration authoring, use `loco_rs::schema::*`:

```rust
use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(
            m,
            "posts",
            &[
                ("title", ColType::StringNull),
                ("content", ColType::StringNull),
            ],
            &[],
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "posts").await
    }
}
```

### Common DSL Operations

**Create table:**
```rust
create_table(m, "table_name", &[(col_name, col_type), ...], &[...refs]).await
```

**Create join table:**
```rust
create_join_table(m, "table_name", &[...cols], &[(model, ""), ...]).await
```

**Add column:**
```rust
add_column(m, "table_name", "col_name", ColType::...Null).await
```

**Add reference:**
```rust
add_column(
    m,
    "table_name",
    "ref_col_name",
    ColType::BigIntegerNull,
).await?;
alter_table(
    m,
    Table::alter()
        .table(TableName::Table)
        .add_foreign_key(
            ForeignKey::create()
                .name("fk-table-ref")
                .from(TableName, TableName::RefCol)
                .to(ReferencedTable, ReferencedTable::Id)
                .to_owned(),
        )
        .to_owned(),
).await
```

**Add index:**
```rust
manager
    .create_index(
        Index::create()
            .name("idx-table-col")
            .table(TableName::Table)
            .col(TableName::Col)
            .to_owned(),
    )
    .await
```

## Validation

```rust
use validator::Validate;

#[derive(Debug, Validate, Deserialize)]
pub struct PostValidator {
    #[validate(length(min = 2, message = "Title must be at least 2 characters long."))]
    pub title: String,
}

impl Validatable for super::_entities::posts::ActiveModel {
    fn validator(&self) -> Box<dyn Validate> {
        Box::new(PostValidator {
            title: self.title.as_ref().cloned().unwrap_or_default(),
        })
    }
}
```

Usage: `post.validate()` — returns `Ok(())` or `Err(validation_errors)`.

## Seeding

Seed files live in `src/fixtures/` as YAML. Connect them in `src/app.rs`:

```rust
async fn seed(ctx: &AppContext, base: &Path) -> Result<()> {
    db::seed::<posts::ActiveModel>(&ctx.db, &base.join("posts.yaml").display().to_string()).await?;
    Ok(())
}
```

Run seeds:
```bash
cargo loco db seed                   # seed
cargo loco db seed --reset           # truncate + seed
cargo loco db seed --dump            # dump tables to fixtures
```

## Relationships

### One-to-Many
```bash
cargo loco generate model company name:string! user:references
```

### Many-to-Many
```bash
cargo loco generate model --link users_votes user:references movie:references vote:int
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

For async-safe isolated databases (PostgreSQL only):
```rust
let boot = boot_test_with_create_db::<App, Migrator>().await;
```

## Configuration

Database settings live in `config/*.yaml`:

```yaml
database:
  uri: "..."
  enable_logging: false
  connect_timeout: 500
  idle_timeout: 500
  min_connections: 1
  max_connections: 1
  auto_migrate: true
  dangerously_truncate: false
  dangerously_recreate: false
```

## Entity Generation Customization

Add to `Cargo.toml`:
```toml
[package.metadata.db.entity]
max-connections = 1
ignore-tables = "table1,table2"
model-extra-derives = "CustomDerive"
```
