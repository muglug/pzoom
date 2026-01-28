<?php
/** @psalm-param ?class-string $s */
function bar(?string $s) : void {}

class A {}

/** @psalm-return ?class-string */
function bat() {
    if (rand(0, 1)) return null;
    return A::class;
}

$a = bat();
$a ? 1 : 0;
bar($a);
