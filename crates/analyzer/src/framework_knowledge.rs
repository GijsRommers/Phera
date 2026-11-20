//! `#[ORM\Column]` properties are read and hydrated by Doctrine.
//! Symfony can invoke an attributed method or inject a property through reflection.

use crate::names::Names;
use phera_ast::{Attribute, UseKind};

pub(crate) mod doctrine;
pub(crate) mod symfony;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AttributeUse {
    ReadWriteProperty,
    WriteProperty,
    InvokeMethod,
    InvokeCommand,
}

use AttributeUse::{InvokeCommand, InvokeMethod, ReadWriteProperty, WriteProperty};

const ATTRIBUTES: &[(&str, AttributeUse)] = &[
    ("Doctrine\\ORM\\Mapping\\Column", ReadWriteProperty),
    ("Doctrine\\ORM\\Mapping\\Id", ReadWriteProperty),
    ("Doctrine\\ORM\\Mapping\\GeneratedValue", WriteProperty),
    ("Doctrine\\ORM\\Mapping\\Version", ReadWriteProperty),
    ("Doctrine\\ORM\\Mapping\\Embedded", ReadWriteProperty),
    ("Doctrine\\ORM\\Mapping\\ManyToOne", ReadWriteProperty),
    ("Doctrine\\ORM\\Mapping\\OneToMany", ReadWriteProperty),
    ("Doctrine\\ORM\\Mapping\\OneToOne", ReadWriteProperty),
    ("Doctrine\\ORM\\Mapping\\ManyToMany", ReadWriteProperty),
    ("Doctrine\\ORM\\Mapping\\JoinColumn", ReadWriteProperty),
    (
        "Doctrine\\ORM\\Mapping\\InverseJoinColumn",
        ReadWriteProperty,
    ),
    ("Doctrine\\ORM\\Mapping\\JoinTable", ReadWriteProperty),
    ("Doctrine\\ORM\\Mapping\\OrderBy", ReadWriteProperty),
    ("Doctrine\\ORM\\Mapping\\PrePersist", InvokeMethod),
    ("Doctrine\\ORM\\Mapping\\PostPersist", InvokeMethod),
    ("Doctrine\\ORM\\Mapping\\PreUpdate", InvokeMethod),
    ("Doctrine\\ORM\\Mapping\\PostUpdate", InvokeMethod),
    ("Doctrine\\ORM\\Mapping\\PreRemove", InvokeMethod),
    ("Doctrine\\ORM\\Mapping\\PostRemove", InvokeMethod),
    ("Doctrine\\ORM\\Mapping\\PostLoad", InvokeMethod),
    ("Doctrine\\ORM\\Mapping\\PreFlush", InvokeMethod),
    (
        "Symfony\\Contracts\\Service\\Attribute\\Required",
        InvokeMethod,
    ),
    (
        "Symfony\\Contracts\\Service\\Attribute\\Required",
        WriteProperty,
    ),
    (
        "Symfony\\Component\\DependencyInjection\\Attribute\\Autowire",
        WriteProperty,
    ),
    (
        "Symfony\\Component\\DependencyInjection\\Attribute\\AutowireIterator",
        WriteProperty,
    ),
    (
        "Symfony\\Component\\DependencyInjection\\Attribute\\AutowireLocator",
        WriteProperty,
    ),
    (
        "Symfony\\Component\\Console\\Attribute\\AsCommand",
        InvokeCommand,
    ),
    (
        "Symfony\\Component\\Routing\\Attribute\\Route",
        InvokeMethod,
    ),
    (
        "Symfony\\Component\\Routing\\Annotation\\Route",
        InvokeMethod,
    ),
    (
        "Symfony\\Component\\EventDispatcher\\Attribute\\AsEventListener",
        InvokeMethod,
    ),
    (
        "Symfony\\Component\\Scheduler\\Attribute\\AsPeriodicTask",
        InvokeMethod,
    ),
    (
        "Symfony\\Component\\Scheduler\\Attribute\\AsCronTask",
        InvokeMethod,
    ),
];

pub(crate) fn attributes_use(
    attributes: &[Attribute],
    names: &Names,
    effect: AttributeUse,
) -> bool {
    attributes.iter().any(|attribute| {
        let resolved = names.resolve(&attribute.name, UseKind::Class);
        ATTRIBUTES.iter().any(|(class, use_kind)| {
            (*use_kind == effect || (*use_kind == ReadWriteProperty && effect == WriteProperty))
                && class.eq_ignore_ascii_case(&resolved[0])
        })
    })
}
