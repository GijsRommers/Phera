//! Compare XMLWriter's PHP APIs with the PHP 8.5 oracle.

use phera_extension_api::testing::scripts::Comparison;

#[test]
fn writer_matrix() {
    let Some(oracle) = Comparison::from_environment("xmlwriter") else {
        return;
    };
    let script = oracle.artifacts.join("writer_matrix.php");
    std::fs::write(&script, MATRIX).expect("write the XMLWriter fixture");
    let matched = oracle
        .compare(&script, |_, _| {})
        .unwrap_or_else(|error| panic!("{error}; outputs in {}", oracle.artifacts.display()));
    let count = matched.stdout.split(|byte| *byte == b'\n').count() - 1;
    assert_eq!(count, 2282);
    eprintln!("{count} XMLWriter cases matched");
}

const MATRIX: &str = r#"<?php
$payloads = [null, '', 'say "hello" & <yes>', "tab\tline\nreturn\r", "é日本🦀", "A\0B", "\xff"];
$names = ['root', 'élément', 'a-b', 'a:local', '', 'bad name', '-1', "ok\0suffix"];
$namespaces = [[null, null], ['p', null], [null, 'urn:x'], ['p','urn:x'], ['', '']];
foreach ([false, true] as $indent) {
    foreach (['', ' ', '  ', "\t"] as $padding) {
        foreach ($names as $name) {
            foreach ($payloads as $text) {
                foreach ($namespaces as [$prefix, $uri]) {
                    $writer = XMLWriter::toMemory();
                    $writer->setIndent($indent);
                    $writer->setIndentString($padding);
                    $writer->startDocument(encoding: 'UTF-8');
                    $writer->startElement('fixture');
                    try {
                        $status = $writer->writeElementNs($prefix, $name, $uri, $text);
                    } catch (Throwable $e) {
                        $status = $e::class . ' ' . $e->getMessage();
                    }
                    $writer->endDocument();
                    echo json_encode([$status, bin2hex($writer->outputMemory())]), "\n";
                }
            }
        }
    }
}
foreach (['UTF-8','ISO-8859-1','UTF-16','UTF-16LE','UTF-16BE','US-ASCII','SHIFT_JIS'] as $encoding) {
    foreach ([false, true] as $indent) {
        $writer = XMLWriter::toMemory();
        $writer->setIndent($indent);
        $writer->startDocument(encoding: $encoding, standalone: 'yes');
        $writer->startDtd('root');
        $writer->writeDtdEntity('local', 'value');
        $writer->writeDtdAttlist('root', 'id ID #IMPLIED');
        $writer->writeDtdElement('root', 'ANY');
        $writer->endDtd();
        $writer->startElement('root');
        $writer->writeAttribute('id', '<&"');
        $writer->writeComment('comment');
        $writer->writeCdata('cdata<');
        $writer->writePi('php', 'echo "test";');
        $writer->writeRaw('&local;');
        $writer->endDocument();
        echo $encoding, ' ', bin2hex($writer->flush(false)), "\n";
        echo $encoding, ' ', bin2hex($writer->flush(true)), "\n";
        echo $encoding, ' ', bin2hex($writer->flush(true)), "\n";
    }
}
"#;

#[test]
fn output_destinations() {
    let Some(oracle) = Comparison::from_environment("xmlwriter-destinations") else {
        return;
    };
    let script = oracle.artifacts.join("destinations.php");
    std::fs::write(&script, DESTINATIONS).expect("write the XMLWriter fixture");
    let matched = oracle
        .compare(&script, |runtime, command| {
            command.current_dir(oracle.scratch_directory(&script, runtime));
        })
        .unwrap_or_else(|error| panic!("{error}; outputs in {}", oracle.artifacts.display()));
    let count = matched.stdout.split(|byte| *byte == b'\n').count() - 1;
    assert_eq!(count, 48);
    eprintln!("{count} XMLWriter destination results matched");
}

const DESTINATIONS: &str = r#"<?php
foreach ([['<a>&', 'é'], ['', "x\r\ny"], ['text', '日本']] as [$attribute, $text]) {
    foreach ([false, true] as $pretty) {
        $file = 'sample file.xml';
        $target = fopen('php://temp', 'w+b');
        foreach (['memory', 'uri', 'stream', 'procedural'] as $mode) {
            $writer = match ($mode) {
                'memory' => XMLWriter::toMemory(),
                'uri' => XMLWriter::toUri('sample%20file.xml'),
                'stream' => XMLWriter::toStream($target),
                'procedural' => xmlwriter_open_memory(),
            };
            xmlwriter_set_indent($writer, $pretty);
            $writer->startDocument(encoding: 'UTF-8');
            $writer->startElement('root');
            xmlwriter_write_attribute($writer, 'a', $attribute);
            echo json_encode($writer->flush(false)), "\n";
            $writer->writeElement('child', $text);
            $writer->endDocument();
            $result = match ($mode) {
                'uri' => file_get_contents($file),
                'stream' => (rewind($target) ? stream_get_contents($target) : false),
                default => xmlwriter_output_memory($writer),
            };
            echo bin2hex($result), "\n";
        }
        unlink($file);
        fclose($target);
    }
}
"#;
