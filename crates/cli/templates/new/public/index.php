<?php

declare(strict_types=1);

// The front controller: the server sends every request that is not a file
// in this directory here.

use App\Application;
use App\Http\Request;

require dirname(__DIR__) . '/vendor/autoload.php';

$application = new Application();
$application->handle(Request::fromGlobals())->send();
