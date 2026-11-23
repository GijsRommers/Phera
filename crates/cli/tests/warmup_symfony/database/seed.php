<?php

// Creates the test application's SQLite database: php -n seed.php <file>.
$database = new PDO('sqlite:'.$argv[1]);
$database->exec('create table products (id integer primary key autoincrement, name varchar(80) not null, price integer not null)');
$database->exec("insert into products (name, price) values ('Lamp', 1200), ('Desk', 34000), ('Chair', 8900)");
