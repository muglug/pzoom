<?php
class A {}
class B {}

/** @return array{A, B}|null */
function pair(): ?array {
    return rand(0,1) ? [new A, new B] : null;
}

function f(): void {
    [$x, $y] = pair();
    if ($y !== null) {
        if ($x !== null) {
            echo "both";
        }
    }
}
