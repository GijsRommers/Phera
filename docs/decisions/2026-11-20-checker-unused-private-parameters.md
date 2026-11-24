# Respect PHPStan's unused private method parameter setting

Decided by: Design

PHPStan's bleeding-edge include enables `method.unusedParameter` at level 4.
Phera uses the same setting and also accepts `featureToggles.unusedParameters`
in NEON and `unused_parameters` in the Phera check table.

Report parameters of private methods whose bodies give no evidence of their
use. Dynamic variable reads, argument introspection, and documented assertions
can use a parameter without a named variable read. The rule leaves those cases
to the analyzer with that information.
