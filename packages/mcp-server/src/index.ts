/**
 * AgentDock MCP server draft.
 *
 * Phase 1 keeps this deliberately thin. The durable implementation will talk
 * to the local AgentDock daemon over a versioned local API instead of executing
 * process-management operations directly from Node.
 */
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";

const server = new McpServer({
  name: "agentdock",
  version: "0.1.0",
});

server.tool(
  "agentdock_status",
  "Check whether the AgentDock MCP bridge is installed and reachable.",
  {},
  async () => ({
    content: [
      {
        type: "text",
        text: JSON.stringify({
          status: "bootstrap",
          daemonApi: "not-yet-connected",
          nextTools: [
            "list_services",
            "reserve_port",
            "release_port",
            "get_project",
            "get_logs",
            "cleanup_orphans"
          ]
        }, null, 2),
      },
    ],
  }),
);

const transport = new StdioServerTransport();
await server.connect(transport);
