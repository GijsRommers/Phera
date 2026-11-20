//! Compare XMLReader cursor events, properties and subtrees with PHP 8.5.

use phera_extension_api::testing::scripts::Comparison;

#[test]
fn xmlreader_matrix() {
    let Some(oracle) = Comparison::from_environment("xmlreader") else {
        return;
    };
    let script = oracle.artifacts.join("xmlreader_matrix.php");
    std::fs::write(&script, MATRIX).expect("write the XMLReader fixture");
    let matched = oracle
        .compare(&script, |runtime, command| {
            command.current_dir(oracle.scratch_directory(&script, runtime));
        })
        .unwrap_or_else(|error| panic!("{error}; outputs in {}", oracle.artifacts.display()));
    let count = matched.stdout.split(|byte| *byte == b'\n').count() - 1;
    assert_eq!(count, 360);
    eprintln!("{count} XMLReader events matched");
}

const MATRIX: &str = r#"<?php
$documents = [
    '<root><item id="1">value</item><item id="2"/><explicit></explicit></root>',
    '<?xml version="1.0"?><r><!--comment--><![CDATA[<data>]]><?p code?><a/></r>',
    '<r xmlns:p="urn:p" xml:lang="en"><p:item p:active="yes">é</p:item><c><p:item/></c></r>',
    "<r>\n <empty/>\n <pair>text</pair>\n</r>",
];
foreach ($documents as $xml) {
    foreach ([0, LIBXML_NOCDATA, LIBXML_NOBLANKS] as $flags) {
        foreach (['string', 'legacy', 'uri', 'stream'] as $mode) {
            $stream = null;
            if ($mode === 'legacy') {
                $reader = new XMLReader();
                $reader->XML($xml, flags: $flags);
            } elseif ($mode === 'uri') {
                file_put_contents('reader.xml', $xml);
                $reader = XMLReader::fromUri('reader.xml', flags: $flags);
            } elseif ($mode === 'stream') {
                $stream = fopen('php://temp', 'w+b');
                fwrite($stream, $xml);
                rewind($stream);
                $reader = XMLReader::fromStream($stream, flags: $flags);
            } else {
                $reader = XMLReader::fromString($xml, flags: $flags);
            }
            while ($reader->read()) {
                $row = [
                    $reader->name, $reader->localName, $reader->nodeType, $reader->depth,
                    $reader->isEmptyElement, $reader->hasValue, $reader->attributeCount,
                    $reader->namespaceURI, $reader->prefix, $reader->xmlLang,
                    $reader->readString(), $reader->readInnerXml(), $reader->readOuterXml(),
                ];
                if ($reader->nodeType === XMLReader::ELEMENT) {
                    $expanded = $reader->expand();
                    $row[] = [$expanded->nodeName, $expanded->textContent];
                    $attrs = [];
                    while ($reader->moveToNextAttribute()) {
                        $attrs[] = [$reader->name, $reader->value, $reader->depth];
                    }
                    $reader->moveToElement();
                    $row[] = $attrs;
                }
                echo json_encode($row), "\n";
            }
            $reader->close();
            if ($stream !== null) {
                fclose($stream);
            }
            if ($mode === 'uri') {
                unlink('reader.xml');
            }
        }
    }
}
"#;
