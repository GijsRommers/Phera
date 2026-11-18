//! File protocol calls run on a small tree shared by both oracle runs.

use super::{Cases, HOSTILE, methods, push};

pub(super) fn info() -> Cases {
    let mut cases = Cases::new();
    for path in HOSTILE
        .iter()
        .copied()
        .chain(["__FILE__", "'/'", "'/tmp///'", "'a//b/'"])
    {
        push(
            &mut cases,
            "info-constructor",
            "",
            &format!("(new SplFileInfo({path}))->getPathname()"),
        );
    }
    for path in [
        "__FILE__",
        "''",
        "'/'",
        "'part//file.log/'",
        "'missing-spl-entry'",
        "'.abc'",
    ] {
        methods(
            &mut cases,
            &format!("$o = new SplFileInfo({path});"),
            &[
                "$o->getPath()",
                "$o->getFilename()",
                "$o->getExtension()",
                "$o->getBasename()",
                "$o->getBasename('.log')",
                "$o->getPathname()",
                "$o->__toString()",
                "$o->__debugInfo()",
                "$o->getFileInfo()->getPathname()",
                "$o->getPathInfo()?->getPathname()",
                "$o->getRealPath()",
                "$o->isFile()",
                "$o->isDir()",
                "$o->isLink()",
                "$o->getType()",
                "$o->getSize()",
                "$o->getLinkTarget()",
            ],
        );
    }
    methods(
        &mut cases,
        "$o = new SplFileInfo(__FILE__);",
        &[
            "$o->getPerms() === fileperms(__FILE__)",
            "$o->getInode() === fileinode(__FILE__)",
            "$o->getOwner() === fileowner(__FILE__)",
            "$o->getGroup() === filegroup(__FILE__)",
            "$o->getATime() === fileatime(__FILE__)",
            "$o->getMTime() === filemtime(__FILE__)",
            "$o->getCTime() === filectime(__FILE__)",
            "$o->isReadable()",
            "$o->isWritable()",
            "$o->isExecutable()",
            "(clone $o)->getPathname()",
            "(array)$o",
        ],
    );
    for class in HOSTILE
        .iter()
        .copied()
        .chain(["'SplFileInfo'", "'UnknownFileInfo'"])
    {
        for method in ["getFileInfo", "getPathInfo", "setInfoClass", "setFileClass"] {
            push(
                &mut cases,
                method,
                "$o = new SplFileInfo(__FILE__);",
                &format!("$o->{method}({class})"),
            );
        }
    }
    cases
}

const TREE: &str = concat!(
    "$root = __DIR__ . '/spl-oracle-directory'; if (!is_dir($root)) {",
    "mkdir($root); mkdir($root . '/dir'); file_put_contents($root . '/example.php', '<?php'); }",
);

pub(super) fn directory(class: &str) -> Cases {
    let mut cases = Cases::new();
    for path in HOSTILE.iter().copied().chain(["__FILE__", "$root"]) {
        push(
            &mut cases,
            "constructor",
            TREE,
            &format!("(new {class}({path}))->getPath()"),
        );
    }
    let pattern = if class == "GlobIterator" {
        "$root . '/example.*'"
    } else {
        "$root . '/dir'"
    };
    let setup = format!("{TREE} $o = new {class}({pattern});");
    methods(
        &mut cases,
        &setup,
        &[
            "$o->getPath()",
            "$o->getPathname()",
            "$o->getFilename()",
            "$o->getExtension()",
            "$o->getBasename()",
            "$o->isDot()",
            "$o->valid()",
            "$o->key()",
            "$o->__debugInfo()",
            "[$o->rewind(), $o->valid(), $o->key()]",
            "get_class(clone $o)",
            "$o->getFileInfo()->getPathname()",
            "$o->getPathInfo()?->getPathname()",
        ],
    );
    for position in HOSTILE {
        push(
            &mut cases,
            "seek",
            &setup,
            &format!("[$o->seek({position}), $o->key()]"),
        );
    }
    if class != "DirectoryIterator" {
        for flags in HOSTILE {
            push(
                &mut cases,
                "flags",
                &setup,
                &format!("[$o->setFlags({flags}), $o->getFlags(), $o->key()]"),
            );
        }
        methods(
            &mut cases,
            &setup,
            &[
                "$o->getFlags()",
                "[$o->setFlags(32),$o->current()]",
                "[$o->setFlags(16),$o->current() === $o]",
            ],
        );
    }
    if class == "RecursiveDirectoryIterator" {
        methods(
            &mut cases,
            &setup,
            &[
                "$o->getSubPath()",
                "$o->getSubPathname()",
                "$o->hasChildren()",
            ],
        );
    }
    if class == "GlobIterator" {
        methods(&mut cases, &setup, &["count($o)", "$o->count()"]);
    }
    cases
}

pub(super) fn stream(temp: bool) -> Cases {
    let mut cases = Cases::new();
    if temp {
        for limit in HOSTILE {
            push(
                &mut cases,
                "temp-constructor",
                "",
                &format!("(new SplTempFileObject({limit}))->getPathname()"),
            );
        }
    } else {
        for path in HOSTILE.iter().copied().chain(["__FILE__", "__DIR__"]) {
            push(
                &mut cases,
                "file-constructor",
                "",
                &format!("(new SplFileObject({path}))->getPathname()"),
            );
        }
        for mode in HOSTILE.iter().copied().chain(["'r'", "'rb'", "'r+'"]) {
            push(
                &mut cases,
                "mode",
                "",
                &format!("(new SplFileObject(__FILE__, {mode}))->getPathname()"),
            );
        }
        methods(
            &mut cases,
            "$o = new SplFileInfo(__FILE__);",
            &[
                "$o->openFile()->getPathname()",
                "$o->openFile(useIncludePath:true)->getPathname()",
                "$o->openFile('invalid')",
                "$o->openFile(context:[])",
            ],
        );
    }
    let instance = if temp {
        "new SplTempFileObject"
    } else {
        "new SplFileObject(__DIR__ . '/spl-oracle-file', 'w+b')"
    };
    let setup = format!("$o={instance}; $o->fwrite(\"one\\n\\ntwo\\r\\nlast\"); $o->rewind();");
    methods(
        &mut cases,
        &setup,
        &[
            "$o->getPathname()",
            "$o->eof()",
            "$o->valid()",
            "$o->current()",
            "$o->fgets()",
            "$o->getCurrentLine()",
            "$o->__toString()",
            "$o->key()",
            "$o->hasChildren()",
            "$o->getChildren()",
            "$o->rewind()",
            "$o->fflush()",
            "$o->ftell()",
            "$o->fgetc()",
            "$o->fstat()['size']",
            "$o->ftruncate(2)",
            "[$o->fseek(2),$o->ftell(),$o->fgets()]",
            "[$o->fgetc(),$o->fgetc(),$o->fgetc(),$o->fgetc(),$o->key()]",
            "[$o->next(),$o->getCurrentLine(),$o->key()]",
            "[$o->fgets(),$o->fgets(),$o->fgets(),$o->fgets(),$o->fgets(),$o->fgets()]",
            "$o->getCsvControl()",
            "$o->fgetcsv()",
            "$o->fputcsv(['a,b',7])",
            "[$o->setCsvControl(';','\"',''),$o->getCsvControl(),$o->fgetcsv()]",
            "[$o->setCsvControl(escape: ''), $o->getCsvControl(), $o->fgetcsv()]",
            "[$o->setCsvControl(';','\"',''), $o->fgetcsv(escape: '')]",
            "$o->fputcsv(['a,b',7], escape: '')",
            "$o->fscanf('%s')",
            "[$o->fscanf('%s',$out),$out]",
            "clone $o",
        ],
    );
    push(
        &mut cases,
        "passthru",
        &format!("$o={instance}; $o->fwrite('abc'); $o->rewind();"),
        "$o->fpassthru()",
    );
    for number in HOSTILE {
        for (method, observation) in [
            ("setFlags", "$o->getFlags()"),
            ("setMaxLineLen", "$o->getMaxLineLen()"),
            ("seek", "$o->key()"),
            ("fseek", "$o->ftell()"),
        ] {
            push(
                &mut cases,
                method,
                &setup,
                &format!("[$o->{method}({number}),{observation}]"),
            );
        }
    }
    for length in ["-12", "0", "1", "4", "1000000", "[]", "1.5", "null"] {
        for method in ["fread", "ftruncate"] {
            push(
                &mut cases,
                method,
                &setup,
                &format!("$o->{method}({length})"),
            );
        }
        push(
            &mut cases,
            "fwrite",
            &setup,
            &format!("$o->fwrite('abc',{length})"),
        );
    }
    for delimiter in ["','", "':'", "''", "'xx'", r#""\xff""#, "[]"] {
        push(
            &mut cases,
            "setcsv",
            &setup,
            &format!("[$o->setCsvControl({delimiter},'\"',''),$o->getCsvControl()]"),
        );
        push(
            &mut cases,
            "getcsv",
            &setup,
            &format!("$o->fgetcsv({delimiter})"),
        );
    }
    for flags in [0, 1, 2, 3, 5, 7, 8, 11, 15] {
        push(
            &mut cases,
            "file-iteration",
            &setup,
            &format!("[$o->setFlags({flags}),iterator_to_array($o)]"),
        );
    }
    cases
}
