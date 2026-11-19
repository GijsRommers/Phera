//! `proc_open(['printf', '%s', $value], ...)` and the shell helpers share PHP's I/O conventions.

use super::{Cases, input};

#[tokio::test(flavor = "current_thread")]
async fn shell_helpers_match_php() {
    let mut cases = Cases::new();
    let commands = [
        b"".as_slice(),
        b" ",
        b"printf hello",
        b"printf 'first\\nsecond  \\t\\r\\n\\n'",
        b"printf 'trailing\\t\\013\\014  '",
        b"printf 'a\\000b\\000'",
        b"printf 'first\\nsecond'",
        b"printf '\\377\\300'",
        b"printf '%s\\n' \"group of words\" \"isn't split\"",
        b"printf '%s' 'quoted \"word\"'",
        b"exit 7",
        b"printf warning >&2; exit 3",
        b"phera-process-absent-program 2>/dev/null",
        b":\0printf unreachable",
        b"printf \\xFF",
        b"printf 'x\xff'",
    ];
    let mut expressions = commands
        .iter()
        .map(|bytes| input(bytes))
        .collect::<Vec<_>>();
    expressions.extend(
        [
            "null",
            "false",
            "true",
            "42",
            "-1",
            "1.23",
            "[]",
            "['test']",
            "new stdClass",
            "INF",
            "NAN",
            "PHP_INT_MAX",
            "PHP_INT_MIN",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    for command in expressions {
        cases.result(format!("shell_exec({command})"));
        for function in ["system", "passthru"] {
            cases.check(format!(
                "$code = 123; $result = {function}({command}, $code); return [$result, $code];"
            ));
        }
        for initial in [
            "null",
            "[]",
            "['marker']",
            "3",
            "false",
            "['key' => 'value']",
        ] {
            cases.check(format!(
                "$out = {initial}; $code = -123; $last = exec({command}, $out, $code);                  return [$last, $out, $code];"
            ));
        }
    }
    for mode in [
        "r", "rb", "br", "rbb", "rw", "", "x", "r\0", "w", "wb", "bw", "null",
    ] {
        let mode = if mode == "null" {
            mode.to_owned()
        } else {
            input(mode.as_bytes())
        };
        cases.check(format!(
            "$p = popen('exit 11', {mode}); return $p === false ? false : pclose($p);"
        ));
    }
    cases.check("$p = popen('printf line', 'r'); return [fgets($p), feof($p), pclose($p)];".into());
    cases.check(
        "$p = popen('/bin/cat >/dev/null', 'w');          return [fwrite($p, 'read until EOF'), pclose($p)];".into(),
    );
    for name in [
        "exec",
        "system",
        "passthru",
        "shell_exec",
        "popen",
        "pclose",
        "proc_nice",
    ] {
        cases.result(format!("{name}()"));
    }
    for input in ["0", "false", "null", "[]", "'zero'", "0.5"] {
        cases.result(format!("proc_nice({input})"));
    }
    cases.compare("unix-shell-functions").await;
}

#[tokio::test(flavor = "current_thread")]
async fn process_descriptors_and_status_match_php() {
    let mut cases = Cases::new();
    for command in [
        "['/usr/bin/printf', '%s|%s', 'two words', \"isn't shell interpreted\"]",
        "'printf \"%s|%s\" \"two words\" \"isn\\\'t shell interpreted\"'",
        "['/bin/sh', '-c', 'printf stdout; printf stderr >&2; exit 23']",
        "''",
        "'printf x\\0'",
        "['/bin/sh', '-c', 'printf \"%s\" \"$ONE-$TWO\"; pwd']",
        "['/bin/sh', '-c', 'printf x; kill -TERM $$']",
        "['/phera-process-program-does-not-exist']",
        "'phera-process-program-does-not-exist 2>/dev/null'",
        "[]",
        "['']",
        "[null]",
        "['/bin/echo', \"contains\\0nul\"]",
        "[100]",
        "null",
        "false",
        "new stdClass",
    ] {
        for env in ["null", "[]", "['ONE' => 42, 'TWO' => 'two words']"] {
            for cwd in ["null", "'/'"] {
                cases.check(format!(
                    "$p = proc_open({command}, [0=>['null'], 1=>['pipe','w'], 2=>['pipe','w']],                      $pipes, {cwd}, {env}, ['bypass_shell'=>true]);                      if (!$p) return [$p, $pipes];                      return [stream_get_contents($pipes[1]), stream_get_contents($pipes[2]),                      proc_close($p), is_resource($pipes[1])];"
                ));
            }
        }
    }
    for descriptors in [
        "['1a'=>['pipe','w']]",
        "[false]",
        "[0=>[]]",
        "[['pipe']]",
        "[['bogus']]",
        "[['file']]",
        "[['file', '/dev/null']]",
        "[['redirect']]",
        "[['redirect', '0']]",
        "[['redirect', 59]]",
        "[['redirect', true]]",
        "[['pipe', null]]",
        "[['pipe', true]]",
        "[['null']]",
        "[1 => ['pipe', 'bad']]",
        "[1 => fopen('php://memory', 'w+')]",
        "[1 => opendir('.')]",
        "null",
        "17",
    ] {
        cases.check(format!(
            "$pipes = 'unchanged'; $p = proc_open('exit 0', {descriptors}, $pipes);              return [$p ? proc_close($p) : $p, is_array($pipes) ? array_keys($pipes) : $pipes];"
        ));
    }
    for code in [0, 1, 23, 255] {
        cases.check(format!(
            "$p = proc_open('exit {code}', [1 => ['pipe','w']], $pipes);              stream_get_contents($pipes[1]);              do {{ $one = proc_get_status($p); }} while ($one['running']);              $two = proc_get_status($p);              $one['pid'] = $one['pid'] > 0; $two['pid'] = $two['pid'] > 0;              return [$one, $two, proc_close($p)];"
        ));
    }
    cases.check(
        "$child = proc_open(['/bin/cat'], [0 => ['pipe','r'], 1 => ['pipe','w']], $pipes);          $n = fwrite($pipes[0], 'stdin until EOF'); fclose($pipes[0]);          return [$n, stream_get_contents($pipes[1]), proc_close($child)];".into(),
    );
    cases.check(
        "$child = proc_open(['/bin/sh', '-c', 'printf abc >&3; printf xyz >&2'],           [1 => ['pipe','w'], 2 => ['redirect', 1], 3 => ['pipe','w']], $pipes);          return [stream_get_contents($pipes[3]), stream_get_contents($pipes[1]), proc_close($child)];"
            .into(),
    );
    cases.check(
        "$child = proc_open(['/bin/sh', '-c', 'printf output; printf discarded >&2'],           [1 => ['file', '/dev/null', 'w'], 2 => fopen('/dev/null', 'w')], $pipes);          return [$pipes, proc_close($child)];".into(),
    );
    cases.check(
        "$child = proc_open(['/bin/sleep', '10'], [], $pipes);          $sent = proc_terminate($child, 15);          do { $status = proc_get_status($child); } while ($status['running']);          $status['pid'] = $status['pid'] > 0;          return [$sent, $status, proc_close($child)];".into(),
    );
    cases.check(
        "$child = proc_open(['/bin/sh', '-c', 'head -c 5242880 /dev/zero'],                             [1 => ['pipe','w']], $pipes);          $data = stream_get_contents($pipes[1]);          return [strlen($data), sha1($data), proc_close($child)];".into(),
    );
    cases.check(
        "$child = proc_open(['/bin/sh', '-c', 'printf one; printf two >&2'],                             [1 => ['pipe','w'], 2 => ['socket']], $pipes);          foreach ($pipes as $p) { stream_set_blocking($p, false); }          $out = [1=>'',2=>'']; $other = null;          while ($pipes) { $read = $pipes; stream_select($read, $other, $other, 2);            foreach ($read as $k=>$p) { $out[$k] .= fread($p, 8192);              if (feof($p)) { fclose($p); unset($pipes[$k]); } } }          return [$out, proc_close($child)];".into(),
    );
    for function in ["proc_terminate", "proc_close", "proc_get_status", "pclose"] {
        for arg in [
            "",
            "null",
            "floatval(1)",
            "new stdClass",
            "false",
            "[]",
            "'invalid'",
        ] {
            cases.result(format!("{function}({arg})"));
        }
    }
    cases.compare("unix-process-descriptors").await;
}
