# Fold null returns with the ownership facts

Date 2026-11-23.

A load of null immediately before its return can become ReturnNull. The existing ownership proof must show that the load overwrites an empty or scalar temporary. Both instructions must be in the same basic block, and no handler may read the temporary after the return. This preserves the order of releases and the return's source line.

| Structure | Was | Is |
|---|---|---|
| Plain null return with a proof | One null load and one register return | Existing ReturnNull |
| Control-flow proof | Forward ownership and backward liveness remove redundant releases | The same facts also prove the null return can omit its temporary |
| By-reference, checked finally and generator returns | Dedicated return instructions | The same instructions |

A local or an unproven temporary can hold a value that must be released before the return. Such loads stay. A jump to the return alone can skip its load, so that pair also stays. Folding before method-site formation lets the existing instruction compaction update branches and handlers.
