<?php
class Ident10 { public string $name = ''; }
class Arg10 { public ?Ident10 $name = null; }
function f(Arg10 $arg): void {
    /** @psalm-fixme RiskyTruthyFalsyComparison */
    if ($arg->name->name ?? null !== "name") {
        return;
    }
    echo "ok";
}
