<?php
// Run with php -n -d extension=/path/to/redis.so generate-ini.php > ini.tsv.
// The output includes session so its definitions become visible when Phera loads it.
$modules = ['standard', 'date', 'filter', 'mbstring', 'iconv', 'pcre', 'pdo_mysql',
    'sqlite3', 'redis', 'session'];
$all = ini_get_all();
$owners = [];
foreach (get_loaded_extensions() as $extension) {
    $module = strtolower($extension);
    if ($module === 'core') {
        continue;
    }
    foreach (ini_get_all($module) as $name => $_) {
        $owners[$name] = $module;
    }
}
foreach ($modules as $module) {
    if (!extension_loaded($module)) {
        fwrite(STDERR, "Load $module in the PHP oracle before generating the catalog.\n");
        exit(1);
    }
}

if (($argv[1] ?? null) === '--functions') {
    $modules = [...$modules, 'core', 'spl', 'tokenizer', 'ctype', 'json', 'pdo',
        'pdo_pgsql', 'pdo_sqlite', 'hash', 'random', 'reflection'];
    foreach ($modules as $module) {
        foreach (get_extension_funcs($module) ?: [] as $function) {
            echo $module, "\t", $function, "\n";
        }
    }
    exit;
}

$booleans = explode(' ', 'allow_url_fopen allow_url_include assert.active assert.bail '
    . 'assert.exception assert.warning auto_detect_line_endings auto_globals_jit '
    . 'display_startup_errors enable_dl enable_post_data_reading expose_php fatal_error_backtraces '
    . 'file_uploads html_errors ignore_repeated_errors ignore_repeated_source ignore_user_abort '
    . 'implicit_flush log_errors mail.add_x_header mail.mixed_lf_and_crlf mbstring.encoding_translation '
    . 'mbstring.strict_detection pcre.jit register_argc_argv report_memleaks report_zend_debug '
    . 'short_open_tag sqlite3.defensive xmlrpc_errors zend.detect_unicode zend.enable_gc '
    . 'zend.exception_ignore_args zend.multibyte zend.signal_check session.auto_start '
    . 'session.cookie_httponly session.cookie_partitioned session.cookie_secure session.lazy_write '
    . 'session.upload_progress.cleanup session.upload_progress.enabled session.use_cookies '
    . 'session.use_only_cookies session.use_strict_mode session.use_trans_sid redis.arrays.autorehash '
    . 'redis.arrays.consistent redis.arrays.index redis.arrays.lazyconnect redis.arrays.pconnect '
    . 'redis.clusters.cache_slots redis.clusters.persistent redis.pconnect.echo_check_liveness '
    . 'redis.pconnect.pool_detect_dirty redis.pconnect.pooling_enabled redis.session.early_refresh '
    . 'redis.session.lock_failure_readonly redis.session.locking_enabled');
$sizes = explode(' ', 'memory_limit max_memory_limit post_max_size upload_max_filesize '
    . 'realpath_cache_size fiber.stack_size zend.max_allowed_stack_size zend.reserved_stack_size');
$lists = explode(' ', 'arg_separator.input disable_functions include_path open_basedir '
    . 'url_rewriter.hosts url_rewriter.tags session.trans_sid_hosts session.trans_sid_tags '
    . 'mbstring.detect_order mbstring.http_input redis.arrays.hosts redis.arrays.names '
    . 'redis.arrays.previous redis.clusters.seeds');
$decimal = explode(' ', 'date.default_latitude date.default_longitude date.sunrise_zenith '
    . 'date.sunset_zenith memory_hard_limit_percent redis.arrays.connecttimeout '
    . 'redis.arrays.readtimeout redis.clusters.read_timeout redis.clusters.timeout');
$integers = ['error_reporting', 'filter.default_flags', 'mbstring.substitute_character'];

$storageOnly = ($argv[1] ?? null) === '--storage-only';
$applied = explode(' ', 'arg_separator.output assert.active assert.exception assert.warning '
    . 'date.timezone default_charset display_errors error_reporting fiber.stack_size ignore_user_abort '
    . 'include_path internal_encoding output_encoding mbstring.detect_order '
    . 'mail.add_x_header mail.cr_lf_mode mail.force_extra_parameters mail.log mail.mixed_lf_and_crlf sendmail_path '
    . 'mbstring.http_output mbstring.internal_encoding mbstring.language mbstring.substitute_character '
    . 'pcre.backtrack_limit pcre.jit pcre.recursion_limit precision serialize_precision '
    . 'unserialize_callback_func unserialize_max_depth zend.assertions zend.exception_ignore_args '
    . 'file_uploads log_errors max_file_uploads max_input_vars max_multipart_body_parts '
    . 'post_max_size upload_max_filesize session.cache_expire session.cache_limiter '
    . 'session.cookie_domain session.cookie_httponly session.cookie_lifetime '
    . 'session.cookie_partitioned session.cookie_path session.cookie_samesite '
    . 'session.cookie_secure session.gc_divisor session.gc_maxlifetime '
    . 'session.gc_probability session.lazy_write session.name '
    . 'session.save_handler session.save_path session.serialize_handler '
    . 'session.use_cookies session.use_only_cookies session.use_strict_mode '
    . 'zend.exception_string_param_max_len');
if ($storageOnly) {
    echo "# INI entries without a runtime consumer\n\n";
    echo "These directives report their PHP default or request-local override. Some have PHP\n";
    echo "validation or deprecation diagnostics. They do not configure the corresponding\n";
    echo "engine behavior. A directive is visible only when its owning module is loaded.\n\n";
} else {
    echo '# PHP ', PHP_VERSION, " CLI defaults. Generated by generate-ini.php.\n";
    echo "# module\tname\tkind\taccess\thex bytes (- is null, = is the empty string)\n";
}
foreach ($all as $name => $entry) {
    $module = $owners[$name] ?? 'core';
    // Phera supplies its server cache settings separately.
    if ($module !== 'core' && !in_array($module, $modules, true)) {
        continue;
    }

    if ($storageOnly) {
        if (!in_array($name, $applied, true)) {
            echo "- `$name`\n";
        }
        continue;
    }

    $default = $entry['global_value'];
    $kind = match (true) {
        in_array($name, $booleans, true) => 'bool',
        in_array($name, $sizes, true) => 'quantity',
        in_array($name, $lists, true) => 'list',
        in_array($name, $decimal, true) => 'decimal',
        in_array($name, $integers, true), $default !== null && preg_match('/^-?\d+$/D', $default) => 'int',
        default => 'string',
    };
    $default = match ($name) {
        // Phera does not install PEAR.
        'include_path' => '.',
        // There is no C extension ABI or on-disk extension directory.
        'extension_dir' => '',
        // The host temporary directory and diagnostic sink are chosen by the runtime.
        'sys_temp_dir', 'error_log' => null,
        default => $default,
    };
    printf("%s\t%s\t%s\t%d\t%s\n", $module, $name, $kind, $entry['access'],
        match ($default) { null => '-', '' => '=', default => bin2hex($default) });
}

if (!$storageOnly) {
    // These PHP directives control Phera's own request file cache.
    echo "phera_server\topcache.revalidate_freq\tint\t7\t32\n";
    echo "phera_server\topcache.revalidate_path\tbool\t7\t30\n";
    echo "phera_server\topcache.validate_timestamps\tbool\t7\t31\n";
    // Phera exposes debugger settings without loading the Xdebug extension.
    echo "phera_debugger\txdebug.mode\tlist\t4\t6f6666\n";
    echo "phera_debugger\txdebug.client_host\tstring\t4\t3132372e302e302e31\n";
    echo "phera_debugger\txdebug.client_port\tint\t4\t39303033\n";
    echo "phera_debugger\txdebug.start_with_request\tstring\t4\t64656661756c74\n";
    echo "phera_debugger\txdebug.idekey\tstring\t4\t=\n";
    echo "phera_debugger\txdebug.connect_timeout_ms\tint\t4\t323030\n";
    echo "phera_debugger\txdebug.log\tstring\t4\t=\n";
}
