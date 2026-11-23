<?php

/*
 * Phera's warm-up for Laravel: what its entry script does between booting
 * the application and Phera\snapshot().
 *
 * `phera serve` boots a Laravel application once per worker and answers
 * every request with a private copy of the state the worker had at
 * Phera\snapshot(). Whatever exists at that point is not built again by any
 * request. So this class builds, once per worker, what nearly every request
 * would otherwise build first:
 *
 *   1. the application's Eloquent models: each one is booted and one query
 *      is built for it (nothing is sent to the database), and
 *   2. the framework services Laravel Octane resolves when one of its
 *      workers starts.
 *
 * No request exists while this runs, and nothing of it may be visible to
 * the requests that follow: no user, no session, no open connection.
 * `phera serve --no-service-warmup` turns all of it off.
 */

namespace Phera\Laravel;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Events\Dispatcher;
use Illuminate\Foundation\Application;
use Illuminate\Http\Request;

final class WarmUp
{
    /**
     * The services Octane resolves in a new worker
     * (Laravel\Octane\Octane::defaultServicesToWarm(), Octane 2.x), except
     * `session.store`. A session store draws its random id when it is built,
     * and Octane gives it a new one before every request
     * (Listeners\FlushSessionState). Nothing of Phera runs before a request,
     * so the store is left for the request that needs it.
     */
    private const SERVICES = [
        'auth', 'cache', 'cache.store', 'config', 'cookie', 'db', 'db.factory',
        'db.transactions', 'encrypter', 'files', 'hash', 'log', 'router',
        'routes', 'session', 'translator', 'url', 'view',
    ];

    public static function beforeSnapshot(Application $app): void
    {
        if (($_SERVER['PHERA_SERVICE_WARMUP'] ?? '') === '0') {
            return;
        }

        $hadSessionStore = $app->resolved('session') && $app->make('session')->getDrivers() !== [];

        // Models first, while no request is bound: see bootModels().
        self::bootModels($app);
        self::resolveServices($app);

        // What Octane clears after every request (Worker::handle,
        // Listeners\FlushTemporaryContainerInstances), in case something
        // above made a guard or a scoped instance.
        if ($app->resolved('auth')) {
            $app->make('auth')->forgetGuards();
        }
        $app->forgetScopedInstances();

        // No session store is left behind either (see SERVICES), also not
        // one that a model's boot made on its way to the signed-in user.
        if (! $hadSessionStore && $app->resolved('session')) {
            $app->make('session')->forgetDrivers();
            $app->forgetInstance('session.store');
        }
    }

    /**
     * Boots every model and builds one query for it.
     *
     * Booting a model runs the application's own code: its boot() and
     * booted() methods and those of its traits. Under PHP-FPM that code runs
     * inside a request. Here there is none, and deliberately nothing stands
     * in for it yet: code that asks for the request or the signed-in user
     * fails instead of quietly getting an empty answer that every request
     * would then inherit. A model that fails is put back exactly as it was
     * and boots inside the request, as it always did.
     *
     * The query is built without the model's global scopes, because a scope
     * is application code that is meant to run per query, with a user.
     */
    private static function bootModels(Application $app): void
    {
        foreach (self::modelClasses($app) as $class) {
            $before = self::eloquentState($app);
            try {
                (new $class)->newQueryWithoutScopes()->toSql();
            } catch (\Throwable) {
                self::restoreEloquentState($app, $before);
            }
        }
    }

    /**
     * The application's models, found the way `php artisan model:prune`
     * finds them: the PHP files under app/Models, named by the
     * application's namespace and their path.
     *
     * @return list<class-string<Model>>
     */
    private static function modelClasses(Application $app): array
    {
        $directory = $app->path('Models');
        if (! is_dir($directory)) {
            return [];
        }

        $classes = [];
        try {
            $namespace = $app->getNamespace().'Models\\';
            $files = new \RecursiveIteratorIterator(
                new \RecursiveDirectoryIterator($directory, \FilesystemIterator::SKIP_DOTS)
            );
            foreach ($files as $file) {
                if ($file->getExtension() !== 'php') {
                    continue;
                }
                $relative = substr($file->getPathname(), strlen($directory) + 1, -strlen('.php'));
                $class = $namespace.str_replace(DIRECTORY_SEPARATOR, '\\', $relative);
                if (is_subclass_of($class, Model::class) && ! (new \ReflectionClass($class))->isAbstract()) {
                    $classes[] = $class;
                }
            }
        } catch (\Throwable) {
            // A file that cannot be loaded: the request that needs it says so.
        }

        // A directory lists its files in no particular order. Sorted, every
        // worker and every boot ends in the same state.
        sort($classes);

        return $classes;
    }

    /**
     * What booting a model changes: Eloquent's class-level state (which
     * models are booted, their global scopes and so on, all static
     * properties of Model) and the events that have listeners.
     *
     * @return array{array<string, mixed>, list<string>}
     */
    private static function eloquentState(Application $app): array
    {
        return [
            (new \ReflectionClass(Model::class))->getStaticProperties(),
            self::eventNames($app),
        ];
    }

    /**
     * Puts Eloquent back to an earlier eloquentState(): the static
     * properties get their earlier values, and the events that have gained
     * their first listener since (a model's `eloquent.saving: App\Models\…`)
     * lose it again, so that the same listener is not there twice once the
     * model boots in a request.
     *
     * @param array{array<string, mixed>, list<string>} $state
     */
    private static function restoreEloquentState(Application $app, array $state): void
    {
        [$statics, $events] = $state;

        $model = new \ReflectionClass(Model::class);
        foreach ($statics as $name => $value) {
            $model->setStaticPropertyValue($name, $value);
        }

        foreach (array_diff(self::eventNames($app), $events) as $event) {
            $app->make('events')->forget($event);
        }
    }

    /** @return list<string> */
    private static function eventNames(Application $app): array
    {
        $events = $app->make('events');

        return $events instanceof Dispatcher ? array_keys($events->getRawListeners()) : [];
    }

    /**
     * Resolves the services of the application's `octane.warm` list when it
     * has one, of Octane's default list otherwise.
     */
    private static function resolveServices(Application $app): void
    {
        // Some services take the current request when they are built; the
        // URL generator cannot be built without one. Laravel's console
        // (Bootstrap\SetRequestForConsole) and Octane put a request made
        // from APP_URL in its place, and so does this. It stays bound, as
        // under Octane: the services that hold it asked the container to
        // hand them every later request, and the container does that only
        // while a request is bound. The HTTP kernel binds the real request
        // before any code of the application runs.
        if (! $app->bound('request')) {
            $app->instance('request', Request::create($app->make('config')->get('app.url', 'http://localhost')));
        }

        foreach ($app->make('config')->get('octane.warm', self::SERVICES) as $service) {
            if (! is_string($service) || $service === 'session.store') {
                continue;
            }
            try {
                if ($app->bound($service)) {
                    $app->make($service);
                }
            } catch (\Throwable) {
                // For example the encrypter without APP_KEY. The request
                // that needs the service reports it, as it always did.
            }
        }
    }
}
