<?php
namespace Bar;

function impure() : ?string {
    /** @var int */
    static $i = 0;

    ++$i;

    return $i % 2 ? "hello" : null;
}

/** @psalm-pure */
function filterOdd(array $a) : void {
    impure();
}
