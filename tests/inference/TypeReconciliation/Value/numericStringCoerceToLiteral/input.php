<?php
/** @param "0"|"1" $s */
function foo(string $s) : void {}

function bar(string $s) : void {
    if (is_numeric($s)) {
        foo($s);
    }
}
