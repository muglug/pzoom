<?php
function takesStr(string $x): void {}

function f(string $s): void {
    if (preg_match('/x(y)/', $s, $matches)) {
        takesStr($matches[1]);
    }
    if (preg_match('/x(y)/', $s, $m2, PREG_OFFSET_CAPTURE)) {
        takesStr($m2[1][0]);
    }
}
