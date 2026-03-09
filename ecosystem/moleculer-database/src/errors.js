/*
 * @moleculer/database
 * Copyright (c) 2022 MoleculerJS (https://github.com/moleculerjs/database)
 * MIT Licensed
 */

"use strict";

const { Errors: { MoleculerClientError } } = require("moleculer-rs-client/src/compat");

class EntityNotFoundError extends MoleculerClientError {
	constructor(id) {
		super("Entity not found", 404, "ENTITY_NOT_FOUND", {
			id
		});
	}
}

module.exports = { EntityNotFoundError };
