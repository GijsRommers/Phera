<?php

use App\Models\User;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\App;
use Illuminate\Support\Facades\Auth;
use Illuminate\Support\Facades\Cookie;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Log;
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

// Signs a user in, without a password: the tests only need two users.
Route::get('/login/{id}', function (Request $request, int $id) {
    Auth::login(User::findOrFail($id));
    $request->session()->put('name', "user $id");

    return ['user' => Auth::id()];
});

// What this visitor is to the application.
Route::get('/whoami', fn (Request $request) => [
    'user' => Auth::id(),
    'name' => $request->session()->get('name'),
    'flash' => $request->session()->get('status'),
    'locale' => App::getLocale(),
    'translator' => trans()->getLocale(),
    'carbon' => now()->locale,
    'queued_cookies' => array_keys(Cookie::getQueuedCookies()),
    'log_context' => Log::sharedContext(),
]);

// A request that leaves as much behind as a request can: session data, a
// flash message, a signed-in user, a queued cookie, another locale, log
// context, and work for when the request has ended.
Route::get('/leave-traces', function (Request $request) {
    Auth::login(User::findOrFail(1));
    $request->session()->put('name', 'the warm-up');
    $request->session()->flash('status', 'saved');
    Cookie::queue('seen', 'yes', 10);
    App::setLocale('nl');
    Log::shareContext(['visitor' => 'the warm-up']);
    app()->terminating(fn () => file_put_contents(storage_path('logs/terminated.log'), "ended\n", FILE_APPEND));

    return ['left' => true];
});

Route::get('/login', fn () => ['login' => 'form'])->name('login');

// Only for a signed-in user: anyone else is sent to the login form.
Route::get('/account', fn () => ['user' => Auth::id()])->middleware('auth');

Route::get('/fails', fn () => throw new RuntimeException('this route always fails'));

Route::get('/moved', fn () => redirect('/visit'));

// Fails with a transaction open.
Route::get('/fails-in-transaction', function () {
    DB::beginTransaction();
    DB::table('posts')->count();

    throw new RuntimeException('left a transaction open');
});
