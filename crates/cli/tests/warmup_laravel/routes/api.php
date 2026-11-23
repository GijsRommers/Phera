<?php

use App\Models\Note;
use App\Models\Post;
use App\Models\Stamp;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Http\Request;
use Illuminate\Support\Carbon;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Route;
use Illuminate\Support\Str;

// The routes below are served under /api, without session or cookies.

// Eloquent, with URLs that must carry the request's own host.
Route::get('/posts', fn () => Post::query()->orderBy('id')->paginate(2));

// Everything a request could inherit from the worker's boot. Values that
// differ between any two requests are reported as facts about them, so the
// whole response can be compared with PHP's.
Route::get('/state', function (Request $request) {
    $now = microtime(true);

    return [
        'request' => [
            'is_the_bound_one' => app('request') === $request,
            'host' => $request->getHost(),
            'url' => url('/somewhere'),
            'header' => $request->header('X-Probe'),
            'cookies' => $request->cookies->all(),
        ],
        'user' => auth()->user(),
        'signed_in' => auth()->check(),
        'session_started' => session()->isStarted(),
        'time' => [
            'laravel_start_is_this_request' => $now - LARAVEL_START < 30,
            'request_time_is_this_request' => $now - $_SERVER['REQUEST_TIME_FLOAT'] < 30,
            'carbon_now_is_this_request' => abs(Carbon::now()->getTimestamp() - $now) < 30,
            'carbon_has_test_time' => Carbon::hasTestNow(),
        ],
        'transaction_level' => DB::transactionLevel(),
        'queries_logged' => count(DB::getQueryLog()),
        'locale' => app()->getLocale(),
        'output_buffers' => ob_get_level(),
    ];
});

// Values that must be drawn in the request, never in the worker's boot.
Route::get('/random', fn () => [
    Str::uuid()->toString(),
    random_int(0, PHP_INT_MAX),
    mt_rand(),
    bin2hex(random_bytes(8)),
    session()->getId(),
]);

// When the Stamp model booted (see App\Models\Stamp).
Route::get('/stamp', function () {
    new Stamp;

    return ['booted_at' => Stamp::$bootedAt];
});

// What the worker's boot left in the process, as this request found it.
// Only Phera has such a boot: this answer is not compared with PHP's.
Route::get('/warmed', function () {
    $connections = [];
    foreach (DB::getConnections() as $name => $connection) {
        $connections[$name] = $connection->getRawPdo() instanceof Closure ? 'not opened' : 'opened';
    }

    return [
        'booted_models' => array_keys((new ReflectionClass(Model::class))->getStaticPropertyValue('booted')),
        'note_boot_attempts' => Note::$boots,
        'note_listeners' => count(app('events')->getRawListeners()['eloquent.creating: '.Note::class] ?? []),
        'connections' => $connections,
        'session_stores' => count(app('session')->getDrivers()),
        'guards' => (fn () => array_keys($this->guards))->call(app('auth')),
        'paginator_declared' => class_exists(Illuminate\Pagination\LengthAwarePaginator::class, false),
    ];
});

// A model that can only boot inside a request (see App\Models\Note).
Route::get('/notes', fn () => [
    'sql' => Note::query()->toSql(),
    'creating_listeners' => count(app('events')->getRawListeners()['eloquent.creating: '.Note::class] ?? []),
]);
