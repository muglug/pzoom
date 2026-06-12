<?php
/** @return non-empty-list<string> */
function partsNoHint() { return ["a"]; }
/** @return non-empty-list<string> */
function partsHint(): array { return ["a"]; }

function f(): void {
    $a = partsNoHint();
    assert(!empty($a));
}
function g(): void {
    $b = partsHint();
    assert(!empty($b));
}
