<?php
/** @psalm-suppress MixedArgument */
function foo(array $a) : array {
    /** @psalm-suppress MixedArrayAssignment */
    $a["b"]["c"] = 5;
    /** @psalm-suppress MixedArrayAccess */
    echo $a["b"]["d"];
    echo $a["a"];
    return $a;
}
