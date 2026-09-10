import { randomUUID } from "node:crypto";
import { McpServer } from "@modelcontextprotocol/server";
import { serveStdio } from "@modelcontextprotocol/server/stdio";
import * as z from "zod/v4";

const DAEMON_ADDR = process.env.AGENTDOCK_ADDR ?? "127.0.0.1:7317";
const SESSION_OWNER =
  process.env.AGENTDOCK_SESSION_ID ?? `mcp:${randomUUID()}`;

type JsonObject = Record<string, unknown>;

async function daemonRequest(
  method: "GET" | "POST",
  path: string,
  body?: JsonObject,
): Promise<unknown> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 5_000);

  try {
    const response = await fetch(`http://${DAEMON_ADDR}${path}`, {
      method,
      headers: body ? { "content-type": "application/json" } : undefined,
      body: body ? JSON.stringify(body) : undefined,
      signal: controller.signal,
    });

    const text = await response.text();
    let payload: unknown = text;

    try {
      payload = text.length > 0 ? JSON.parse(text) : null;
    } catch {
      // Preserve non-JSON daemon responses for diagnostics.
    }

    if (!response.ok) {
      throw new Error(
        `AgentDock daemon HTTP ${response.status}: ${JSON.stringify(payload)}`,
      );
    }

    return payload;
  } finally {
    clearTimeout(timeout);
  }
}

function textResult(value: unknown) {
  return {
    content: [
      {
        type: "text" as const,
        text: JSON.stringify(value, null, 2),
      },
    ],
  };
}

function errorResult(error: unknown) {
  const message =
    error instanceof Error ? error.message : String(error);

  return {
    content: [
      {
        type: "text" as const,
        text: message,
      },
    ],
    isError: true,
  };
}

function createServer(): McpServer {
  const server = new McpServer({
    name: "agentdock",
    version: "0.5.0",
  });

  server.registerTool(
    "agentdock_status",
    {
      description:
        "Check the local AgentDock daemon, proxy, and durable registry status.",
      annotations: {
        readOnlyHint: true,
        openWorldHint: false,
      },
    },
    async () => {
      try {
        return textResult(
          await daemonRequest("GET", "/v1/status"),
        );
      } catch (error) {
        return errorResult(error);
      }
    },
  );

  server.registerTool(
    "list_services",
    {
      description:
        "List services discovered by AgentDock, including project, framework, lifecycle, port, and agent attribution.",
      inputSchema: z.object({
        includeAll: z
          .boolean()
          .optional()
          .describe(
            "Include system and unknown listeners in addition to development/infrastructure services.",
          ),
      }),
      annotations: {
        readOnlyHint: true,
        openWorldHint: false,
      },
    },
    async ({ includeAll }) => {
      try {
        const path = includeAll
          ? "/v1/services?all=1"
          : "/v1/services";
        return textResult(
          await daemonRequest("GET", path),
        );
      } catch (error) {
        return errorResult(error);
      }
    },
  );

  server.registerTool(
    "get_project",
    {
      description:
        "Get one AgentDock project by stable project ID or project name.",
      inputSchema: z.object({
        projectId: z.string().min(1).optional(),
        name: z.string().min(1).optional(),
      }),
      annotations: {
        readOnlyHint: true,
        openWorldHint: false,
      },
    },
    async ({ projectId, name }) => {
      if (!projectId && !name) {
        return errorResult(
          new Error("projectId or name is required"),
        );
      }

      try {
        const payload = (await daemonRequest(
          "GET",
          "/v1/projects",
        )) as { projects?: Array<Record<string, unknown>> };

        const projects = payload.projects ?? [];
        const match = projects.find((project) => {
          if (projectId && project.id === projectId) {
            return true;
          }

          const identity = project.identity as
            | Record<string, unknown>
            | undefined;

          return (
            name !== undefined &&
            identity?.name === name
          );
        });

        if (!match) {
          return errorResult(
            new Error("project not found"),
          );
        }

        return textResult(match);
      } catch (error) {
        return errorResult(error);
      }
    },
  );

  server.registerTool(
    "list_routes",
    {
      description:
        "List persistent AgentDock localhost routes and aliases.",
      annotations: {
        readOnlyHint: true,
        openWorldHint: false,
      },
    },
    async () => {
      try {
        return textResult(
          await daemonRequest("GET", "/v1/routes"),
        );
      } catch (error) {
        return errorResult(error);
      }
    },
  );

  server.registerTool(
    "reserve_port",
    {
      description:
        "Reserve an available local development port for this MCP session for 60 seconds.",
      inputSchema: z.object({}),
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async () => {
      try {
        return textResult(
          await daemonRequest(
            "POST",
            "/v1/ports/reserve",
            { owner: SESSION_OWNER },
          ),
        );
      } catch (error) {
        return errorResult(error);
      }
    },
  );

  server.registerTool(
    "release_port",
    {
      description:
        "Release a local port reservation owned by this MCP session. Reservations owned by another session cannot be released.",
      inputSchema: z.object({
        port: z.number().int().min(1).max(65535),
      }),
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ port }) => {
      try {
        return textResult(
          await daemonRequest(
            "POST",
            "/v1/ports/release",
            {
              owner: SESSION_OWNER,
              port,
            },
          ),
        );
      } catch (error) {
        return errorResult(error);
      }
    },
  );

  server.registerTool(
    "get_preview_url",
    {
      description:
        "Get the stable local AgentDock preview URL for a project.",
      inputSchema: z.object({
        projectId: z.string().min(1),
      }),
      annotations: {
        readOnlyHint: true,
        openWorldHint: false,
      },
    },
    async ({ projectId }) => {
      try {
        return textResult(
          await daemonRequest(
            "GET",
            `/v1/preview?project_id=${encodeURIComponent(projectId)}`,
          ),
        );
      } catch (error) {
        return errorResult(error);
      }
    },
  );

  server.registerTool(
    "cleanup_orphans",
    {
      description:
        "Inspect orphaned AgentDock services. Destructive cleanup is intentionally disabled until process ownership is provable.",
      inputSchema: z.object({
        dryRun: z
          .boolean()
          .optional()
          .default(true),
      }),
      annotations: {
        readOnlyHint: false,
        destructiveHint: true,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ dryRun }) => {
      try {
        const payload = (await daemonRequest(
          "GET",
          "/v1/services?all=1",
        )) as {
          services?: Array<Record<string, unknown>>;
        };

        const orphans = (payload.services ?? []).filter(
          (service) => service.state === "orphaned",
        );

        if (!dryRun) {
          return {
            ...textResult({
              executed: false,
              reason:
                "Destructive orphan cleanup remains disabled until AgentDock can prove process/session ownership.",
              orphanedServices: orphans,
            }),
            isError: true,
          };
        }

        return textResult({
          dryRun: true,
          orphanedServices: orphans,
        });
      } catch (error) {
        return errorResult(error);
      }
    },
  );

  return server;
}

void serveStdio(createServer);
console.error(
  `AgentDock MCP server connected to daemon ${DAEMON_ADDR} as ${SESSION_OWNER}`,
);
