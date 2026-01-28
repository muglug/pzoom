<?php
/** @return array{a:mixed} */
function foo(array $r) {
    if (!empty($r["a"])) {}
    return $r;
}
