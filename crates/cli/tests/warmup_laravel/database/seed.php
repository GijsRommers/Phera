<?php

// Creates the test application's SQLite database: php -n seed.php <file>.
$database = new PDO('sqlite:'.$argv[1]);
$database->exec('create table posts (id integer primary key, title text not null, published integer not null default 0, created_at text, updated_at text)');
$database->exec('create table users (id integer primary key, name text not null, email text not null unique, password text not null, remember_token text, created_at text, updated_at text)');
$database->exec('create table notes (id integer primary key, owner_id integer, visible integer not null default 1, text text, created_at text, updated_at text)');
$insert = $database->prepare('insert into posts (title, published, created_at, updated_at) values (?, ?, ?, ?)');
for ($number = 1; $number <= 5; $number++) {
    $insert->execute(["Post $number", $number % 2, '2026-01-01 10:00:00', '2026-01-01 10:00:00']);
}
// Two users, for the tests that sign in. Nobody ever types the password.
$insert = $database->prepare('insert into users (id, name, email, password) values (?, ?, ?, ?)');
$password = password_hash('secret', PASSWORD_BCRYPT, ['cost' => 4]);
$insert->execute([1, 'Ada', 'ada@example.test', $password]);
$insert->execute([2, 'Bob', 'bob@example.test', $password]);
