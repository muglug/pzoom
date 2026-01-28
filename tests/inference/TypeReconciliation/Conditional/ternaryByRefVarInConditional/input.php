<?php
function foo(): void {
    $b = null;
    if (rand(0, 1) || bar($b)) {
        if (is_int($b)) { }
    }
}
function bar(?int &$a): void {
    $a = 5;
}