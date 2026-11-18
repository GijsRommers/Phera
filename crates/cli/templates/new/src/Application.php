<?php

declare(strict_types=1);

namespace App;

use App\Http\Request;
use App\Http\Response;

final class Application
{
    /**
     * The routes: "METHOD /path" and the function that answers it.
     *
     * @return array<string, callable(Request): Response>
     */
    private function routes(): array
    {
        return [
            'GET /' => fn (Request $request): Response => Response::json([
                'application' => $_ENV['APP_NAME'] ?? '{{name}}',
                'environment' => $_ENV['APP_ENV'] ?? 'dev',
                'php' => PHP_VERSION,
            ]),
            'GET /hello' => fn (Request $request): Response => Response::json([
                'hello' => $request->query('name', 'world'),
            ]),
        ];
    }

    public function handle(Request $request): Response
    {
        $route = $this->routes()[$request->method . ' ' . $request->path] ?? null;
        if ($route === null) {
            return Response::json(['error' => 'Not found'], 404);
        }

        return $route($request);
    }
}
