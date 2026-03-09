# Database

Advanced database access service for Moleculer microservices — the `@moleculer/database` module.

**JS Source:** `ecosystem/moleculer-database/`  
**Rust port:** 🔲 Planned  
**NPM:** `@moleculer/database`

## Overview

`@moleculer/database` is a powerful database mixin that provides auto-generated CRUD actions for any adapter. It follows the *one database per service* pattern.

> The Rust port is planned. This page documents the JS module bundled in `ecosystem/moleculer-database/`.

## Features

- Multiple pluggable adapters: **NeDB**, **MongoDB**, **Knex** (PostgreSQL, MySQL, SQLite)
- Auto-generated CRUD actions with built-in caching
- Pagination, field filtering, field sanitization/validation
- Read-only, immutable, virtual fields
- Field-level permissions (read/write)
- ID field encoding
- Data transformation & population across services
- Create/update/remove lifecycle hooks
- Soft delete mode
- Scopes support
- Entity lifecycle events
- Multi-tenancy

## Install

```bash
npm i @moleculer/database

# Optional adapters:
npm i @seald-io/nedb       # NeDB (embedded, great for prototyping)
npm i mongodb              # MongoDB
npm i knex pg              # Knex + PostgreSQL
npm i knex mysql2          # Knex + MySQL
```

## Basic usage

```js
// posts.service.js
const DbService = require("@moleculer/database").Service;

module.exports = {
    name: "posts",
    mixins: [
        DbService({ adapter: "NeDB" })
    ],

    settings: {
        fields: {
            id:        { type: "string", primaryKey: true, columnName: "_id" },
            title:     { type: "string", max: 255, trim: true, required: true },
            content:   { type: "string" },
            votes:     { type: "number", integer: true, min: 0, default: 0 },
            status:    { type: "boolean", default: true },
            createdAt: { type: "number", readonly: true, onCreate: () => Date.now() },
            updatedAt: { type: "number", readonly: true, onUpdate: () => Date.now() }
        }
    }
};
```

## Auto-generated actions

The mixin automatically creates these RESTful actions:

| Action | HTTP (with API GW) | Description |
|--------|---------------------|-------------|
| `posts.find` | `GET /posts` | Find entities with filtering |
| `posts.list` | `GET /posts` | Find with pagination |
| `posts.count` | `GET /posts/count` | Count entities |
| `posts.get` | `GET /posts/:id` | Get entity by ID |
| `posts.create` | `POST /posts` | Create new entity |
| `posts.update` | `PUT /posts/:id` | Update entity |
| `posts.replace` | `PUT /posts/:id` | Replace entity |
| `posts.remove` | `DELETE /posts/:id` | Delete entity |

## Adapters

### NeDB (embedded)

```js
DbService({ adapter: "NeDB" })
// Stores data in memory or a file
```

### MongoDB

```js
DbService({
    adapter: {
        type: "MongoDB",
        options: {
            uri: "mongodb://localhost/mydb",
            collection: "posts"
        }
    }
})
```

### Knex (SQL)

```js
DbService({
    adapter: {
        type: "Knex",
        options: {
            knex: {
                client: "pg",
                connection: process.env.DATABASE_URL,
            },
            tableName: "posts"
        }
    }
})
```

## Field definitions

```js
settings: {
    fields: {
        // Primary key
        id: { type: "string", primaryKey: true, columnName: "_id" },

        // Required string with max length
        title: { type: "string", max: 255, trim: true, required: true },

        // Read-only, set on create
        createdAt: {
            type: "number",
            readonly: true,
            onCreate: () => Date.now()
        },

        // Computed virtual field (not stored)
        fullName: {
            type: "string",
            virtual: true,
            get: ({ entity }) => `${entity.firstName} ${entity.lastName}`
        },

        // Field with permissions
        password: {
            type: "string",
            hidden: true,              // never returned
            set: ({ value }) => bcrypt.hash(value, 10)
        }
    }
}
```

## Populating (cross-service joins)

```js
settings: {
    fields: {
        authorId: { type: "string" },
        author: {
            type: "object",
            virtual: true,
            populate: {
                action: "users.get",
                keyField: "authorId",
            }
        }
    }
}

// Usage:
broker.call("posts.get", { id: "abc", populate: ["author"] })
// Returns post with nested author object fetched via users.get
```

## Scopes

Scopes are predefined query filters that can be applied automatically:

```js
settings: {
    scopes: {
        // Only return active records
        active: { status: true },

        // Only return non-deleted records (soft delete)
        notDeleted: { deletedAt: null }
    },

    // Apply these scopes to all queries by default
    defaultScopes: ["notDeleted"]
}
```

## Multi-tenancy

```js
module.exports = {
    name: "posts",
    mixins: [DbService()],

    methods: {
        // Return a different adapter/collection per tenant
        getAdapterByContext(ctx) {
            const tenantId = ctx.meta.tenantId;
            return {
                type: "MongoDB",
                options: { uri: `mongodb://localhost/tenant_${tenantId}` }
            };
        }
    }
};
```

## Source

The complete implementation is at `ecosystem/moleculer-database/`. See the [README](https://github.com/oneErrortime/mj/blob/main/ecosystem/moleculer-database/README.md) and the extensive test suite at `ecosystem/moleculer-database/test/`.
