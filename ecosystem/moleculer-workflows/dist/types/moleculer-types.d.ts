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
//# sourceMappingURL=moleculer-types.d.ts.map