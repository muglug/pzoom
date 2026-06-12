<?php
function foo(array $n): void {
    if (empty($n)) {
        return;
    }
    while (!empty($n)) {
        unset($n[rand(0, 10)]);
    }
}
