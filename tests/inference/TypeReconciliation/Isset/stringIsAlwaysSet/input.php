<?php
function foo(string $s) : string {
    if (!isset($s)) {
        return "foo";
    }
    return "bar";
}
