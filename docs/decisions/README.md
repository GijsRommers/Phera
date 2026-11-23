# Decisions

Every decision that shapes Phera is written down here, one file per decision:
what was decided, why, and what was turned down.

- A pull request that takes a decision adds a file in the same pull request,
  named `YYYY-MM-DD-a-few-words.md`, with the headings of the existing files.
  One file per decision means two pull requests never edit the same lines.
- "Decided by: Owner" means Gijs Rommers decided it. "Design" means it was
  decided during design or review work, and the owner can overrule it.
- A decision that is reversed keeps its file and gains a line at the top that
  names the file that replaces it.

The areas are: scope and compatibility, safety and dependencies, server and
requests, tools, speed and method, process. `ls docs/decisions` lists them by
date.
