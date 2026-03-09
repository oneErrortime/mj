/**
 * moleculer-types.ts
 *
 * Augments the types exported by moleculer-compat.ts (our stand-in for the
 * `moleculer` package) with the extra properties added by moleculer-workflows.
 */

import type { WorkflowContextProps, WorkflowHandler, WorkflowServiceBrokerMethods } from "./types.ts";
import type { WorkflowSchema } from "./workflow.ts";
import Workflow from "./workflow.ts";

export type { WorkflowSchema };

// Re-export the augmented types so consumers can use them directly.
declare module "./moleculer-compat.ts" {
  interface Context {
    wf: WorkflowContextProps;
  }

  interface ServiceBroker {
    wf: WorkflowServiceBrokerMethods;
  }

  interface Service {
    $workflows?: Workflow[];
  }

  interface ServiceSchema {
    workflows?: {
      [key: string]: WorkflowSchema;
    };
  }

  interface Middleware {
    localWorkflow?: (next: WorkflowHandler) => WorkflowHandler;
  }
}
