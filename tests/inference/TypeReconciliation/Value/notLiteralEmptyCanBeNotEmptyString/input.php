<?php
/**
 * @param non-empty-string $s
 */
function foo(string $s) : void {}

function takesString(string $s) : void {
    if ($s !== "") {
        foo($s);
    }
}