<?php
/** @return non-empty-list<string> */
function p() { return ["a"]; }
function f(int $x): void {
    $a = p();
    assert(!empty($a));
    assert($x > 0);
}
