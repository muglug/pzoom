<?php
class A {}

function foo(string $s) : void {
    if ($s === A::class) {
        bar($s);
    }
}

/** @param class-string $s */
function bar(string $s) : void {
    new $s();
}
