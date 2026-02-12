/**
 * Request and response handlers for the API.
 */

export interface ApiRequest {
  method: string;
  path: string;
  headers: Record<string, string>;
  body?: unknown;
}

export interface ApiResponse {
  status: number;
  headers: Record<string, string>;
  body: unknown;
}

export type HttpMethod = "GET" | "POST" | "PUT" | "DELETE";

export enum StatusCode {
  OK = 200,
  Created = 201,
  BadRequest = 400,
  NotFound = 404,
  InternalError = 500,
}

export interface RouteHandler {
  method: HttpMethod;
  path: string;
  handle(req: ApiRequest): Promise<ApiResponse>;
}

export class Router {
  private routes: RouteHandler[] = [];

  register(handler: RouteHandler): void {
    this.routes.push(handler);
  }

  async dispatch(req: ApiRequest): Promise<ApiResponse> {
    const handler = this.routes.find(
      (r) => r.method === req.method && r.path === req.path
    );

    if (!handler) {
      return {
        status: StatusCode.NotFound,
        headers: {},
        body: { error: "Route not found" },
      };
    }

    return handler.handle(req);
  }
}

export function createJsonResponse(
  status: number,
  body: unknown
): ApiResponse {
  return {
    status,
    headers: { "Content-Type": "application/json" },
    body,
  };
}

export const DEFAULT_TIMEOUT = 30000;
