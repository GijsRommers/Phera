# phera/psr

PSR adapters for Phera's native HTTP and logging classes, in plain PHP.

| PSR | Class | Wraps |
| --- | --- | --- |
| PSR-3 | `Phera\Psr\Psr3Logger` | `Phera\Log\Logger` |
| PSR-7 | `Request`, `ServerRequest`, `Response`, `Stream`, `Uri` | `Phera\Http\Request` / `Response` |
| PSR-15 | `Phera\Psr\Psr15Bridge`, `Pipeline` | `Phera\Http\Router` |
| PSR-17 | `Phera\Psr\HttpFactory` (all five factories) | — |
| PSR-18 | `Phera\Psr\Psr18Client` | `Phera\Http\Client` |

```php
use Phera\Http\{Client, Router};
use Phera\Log\Logger;
use Phera\Psr\{Psr15Bridge, Psr18Client, Psr3Logger};

// A PSR-15 application (Slim, Mezzio, ...) served by `phera serve app.php`:
$router = new Router();
$router->any('/*path', Psr15Bridge::handler($app, new SessionMiddleware()));

// A PSR-3 logger for libraries that ask for one:
$logger = new Psr3Logger(new Logger('app', 'info', 'json', '/var/log/app.log'));

// A PSR-18 client over pooled HTTP/1.1 and HTTP/2 connections:
$http = new Psr18Client(new Client(['timeout' => 5.0]));

return $router;
```

`ServerRequest::fromPhera()` gives route parameters as attributes, parses
the query, cookies and form bodies, and keeps the body as a string stream.
`Response::toPhera()` sends any PSR-7 response with exactly its headers.

## Tests

The tests need no network and no `composer install`: without `vendor/`,
`tests/bootstrap.php` loads the PSR interfaces from `tests/stubs`.

```sh
phera tests/unit.php                     # PSR-3, PSR-7, PSR-17, middleware order
phera serve tests/app.php &              # PSR-15 behind the router
phera tests/client.php http://127.0.0.1:8080   # PSR-18 against it
```

`cargo test -p phera-cli --test http_client` runs all three.
