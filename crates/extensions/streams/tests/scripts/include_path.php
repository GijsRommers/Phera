<?php
mkdir('library');
file_put_contents('library/content.txt', 'found');
file_put_contents('library/message.php', '<?php return "included";');
set_include_path('library');
var_dump(get_include_path());
var_dump(dirname('library/nested/content.txt', 2));
var_dump(file_exists('library/content.txt'), is_file('library/content.txt'));
var_dump(basename(stream_resolve_include_path('content.txt')));
var_dump(file_get_contents('content.txt', true));
$stream = fopen('content.txt', 'r', true);
var_dump(fread($stream, 5), fclose($stream));
var_dump(require 'message.php');
chdir('library');
var_dump(file_get_contents('content.txt'));
var_dump(require './message.php');
