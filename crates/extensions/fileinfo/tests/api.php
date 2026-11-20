$root = $argv[1];
$png = $root . '/sample.png';
$buffer = file_get_contents($png);
$missing = $root . '/missing-file';
$flags = [FILEINFO_NONE, FILEINFO_MIME, FILEINFO_MIME_TYPE,
          FILEINFO_MIME_ENCODING, FILEINFO_EXTENSION, FILEINFO_APPLE, FILEINFO_RAW, -1];
foreach ($flags as $index => $bits) {
    $handle = finfo_open(flags: $bits, magic_database: '');
    run("procedure-buffer-$index", fn () => finfo_buffer($handle, $buffer));
    run("procedure-path-$index", fn () => finfo_file($handle, $png));
    run("procedure-directory-$index", fn () => finfo_file($handle, $root));
    run("object-buffer-$index", fn () => (new finfo($bits))->buffer($buffer));
    run("object-path-$index", fn () => (new finfo($bits))->file($png));
}
$f = new finfo();
run('set-flags', fn () => $f->set_flags(FILEINFO_MIME_TYPE));
run('persistent-flags', fn () => $f->buffer($buffer));
run('override-flags', fn () => $f->buffer($buffer, FILEINFO_EXTENSION));
run('restore-flags', fn () => $f->buffer($buffer));
run('procedure-set-flags', fn () => finfo_set_flags($f, FILEINFO_MIME_ENCODING));
run('persistent-procedure-flags', fn () => finfo_file($f, $png));
run('close', fn () => finfo_close($f));
run('close-type', fn () => finfo_close(null));
run('close-missing', fn () => finfo_close());
run('close-is-noop', fn () => $f->file($png));
run('missing-path', fn () => finfo_file($f, $missing));
run('missing-method-path', fn () => $f->file($missing));
run('missing-mime-path', fn () => mime_content_type($missing));
run('empty-path', fn () => finfo_file($f, ''));
run('empty-method-path', fn () => $f->file(''));
run('empty-mime-path', fn () => mime_content_type(''));
run('nul-path', fn () => $f->file("a\0b"));
run('nul-database', fn () => finfo_open(0, "a\0b"));
run('missing-database', fn () => finfo_open(0, $missing));
run('missing-method-database', fn () => new finfo(0, $missing));
run('nul-mime-path', fn () => mime_content_type("a\0b"));
run('mime-path-type', fn () => mime_content_type(42));
run('object-type', fn () => finfo_buffer(3, 'test'));
run('buffer-type', fn () => $f->buffer([]));
run('flags-type', fn () => $f->buffer('test', []));
run('context-type', fn () => $f->buffer('test', 0, 2));
run('context-deprecated', fn () => $f->buffer('test', 0, null));
run('context-valid', fn () => $f->file($png, 0, stream_context_create()));
run('clone', fn () => clone $f);
run('serialize', fn () => serialize($f));
run('uninitialized', fn () => (new ReflectionClass(finfo::class))
    ->newInstanceWithoutConstructor()->buffer('text'));
run('mime-directory', fn () => mime_content_type($root));
run('mime-stream', function () use ($png) {
    $stream = fopen($png, 'rb');
    fseek($stream, 3);
    return [mime_content_type($stream), ftell($stream), fread($stream, 1)];
});
run('data-wrapper', fn () => mime_content_type('data://text/plain,some%20text'));
