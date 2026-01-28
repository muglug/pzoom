<?php
function foo(): void {
    $b = null;
    $c = rand(0, 1) ? bar($b) : null;
    if (is_int($b)) { }
}
function bar(?int &$a): void {
    $a = 5;
}