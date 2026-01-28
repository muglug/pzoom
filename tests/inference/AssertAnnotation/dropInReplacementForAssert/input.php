<?php
namespace Bar;

/**
 * @param mixed $_b
 * @psalm-assert !falsy $_b
 */
function myAssert($_b) : void {
    if (!$_b) {
        throw new \Exception("bad");
    }
}

function bar(?string $s) : string {
    myAssert($s !== null);
    return $s;
}
