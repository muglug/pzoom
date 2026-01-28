<?php
/** @return array{a:mixed} */
function foo(array $r) {
    if (isset($r["a"]) && $r["a"]) {}
    return $r;
}
