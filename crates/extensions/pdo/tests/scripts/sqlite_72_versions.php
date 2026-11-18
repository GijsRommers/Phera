<?php
$p=PDO::connect('sqlite::memory:');
$version=$p->query('SELECT sqlite_version()')->fetchColumn();
var_dump(is_string($version),$p->getAttribute(PDO::ATTR_CLIENT_VERSION)===$version,
    $p->getAttribute(PDO::ATTR_SERVER_VERSION)===$version);
