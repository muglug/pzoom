<?php
/** @return array{int, string} */
function f(bool $b): ?array {
    return $b ? [1, "a"] : null;
}

$r = f(rand(0, 1) === 1);
if ($r) {
    echo $r[1];
}
