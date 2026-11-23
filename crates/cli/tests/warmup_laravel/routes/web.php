<?php

use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;

// The routes below have Laravel's `web` middleware: encrypted cookies, a
// session and CSRF protection.

// Remembers a name in the session and reports what the session held before.
Route::get('/visit', function (Request $request) {
    $before = $request->session()->get('name');
    if ($request->query('name') !== null) {
        $request->session()->put('name', $request->query('name'));
    }

    return ['before' => $before, 'token_length' => strlen(csrf_token())];
});
