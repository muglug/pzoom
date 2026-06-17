<?php

// A truthy-narrowed `mixed` is `non-empty-mixed`. Psalm models that as a
// subtype of mixed, so calling a method on it is a MixedMethodCall ("cannot
// determine the type"), exactly like plain `mixed` — not an InvalidMethodCall.
function f(mixed $x): void {
    if ($x) {
        $x->foo();
    }
}

function g(mixed $x): void {
    $x && $x->bar();
}
