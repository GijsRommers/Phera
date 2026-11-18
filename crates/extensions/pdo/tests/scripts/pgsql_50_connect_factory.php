<?php
$dsn='pgsql:host=127.0.0.1;port=54329;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$p=PDO::connect($dsn,'phera','phera');var_dump(get_class($p),$p instanceof PDO,$p->query('SELECT 1')->fetchColumn());
