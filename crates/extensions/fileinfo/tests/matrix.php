<?php
$root = $argv[1];
$flags = [FILEINFO_NONE, FILEINFO_MIME_TYPE, FILEINFO_MIME_ENCODING,
          FILEINFO_MIME, FILEINFO_EXTENSION];

foreach (scandir($root) as $name) {
    if ($name === '.' || $name === '..') {
        continue;
    }
    $path = $root . '/' . $name;
    $data = file_get_contents($path);
    foreach ($flags as $flag) {
        $handle = new finfo($flag);
        echo json_encode([$name, $flag, $handle->buffer($data), $handle->file($path)]), "\n";
    }
}
