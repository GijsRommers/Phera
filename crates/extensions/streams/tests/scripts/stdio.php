<?php
echo 'echo-before|';
fwrite(STDOUT, "stdout|");
fprintf(STDERR, "%s=%02d\n", 'stderr', 7);
vfprintf(STDOUT, "%3s:%d|", ['s', 2]);
fputs(fopen('php://output', 'w'), "output|");
echo 'echo-after|';
var_dump(is_resource(STDIN), get_resource_type(STDOUT), stream_isatty(STDERR));
var_dump(fread(STDIN, 1), feof(STDIN), fgets(STDIN), fread(STDIN, 1), feof(STDIN));
