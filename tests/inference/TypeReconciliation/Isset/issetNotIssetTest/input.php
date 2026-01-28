<?php
class B {
    /** @var string */
    public $c = "hello";
}

function foo(array $a, B $b, string $s): void {
    if ($s !== "bar" && !isset($a[$b->c])) {
        return;
    }

    if ($s !== "bar" && isset($a[$b->c])) {
        // do something
    } else {
        // something else
    }
}