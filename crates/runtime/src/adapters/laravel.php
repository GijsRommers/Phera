<?php

// Phera's entry for Laravel, run in place of public/index.php by phera serve:
// the application and its providers boot once per worker before the first
// request, and every request continues after Phera\snapshot() with a copy.
// handleRequest() is on the line of Laravel's own index.php, for its traces.
use Illuminate\Foundation\Application;
use Illuminate\Http\Request;
require __DIR__.'/../vendor/autoload.php';
/** @var Application $app */
$app = require_once __DIR__.'/../bootstrap/app.php';
$app->make(Illuminate\Contracts\Http\Kernel::class)->bootstrap();
Phera\snapshot();
define('LARAVEL_START', microtime(true));
// Determine if the application is in maintenance mode...
if (file_exists($maintenance = __DIR__.'/../storage/framework/maintenance.php')) {
    require $maintenance;
}
// Bootstrap Laravel and handle the request...
$app->handleRequest(Request::capture());
