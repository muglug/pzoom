<?php
function f(callable $c): void {
    $c();
}
/** @var object $o */;

$ca = [$o::class, 'createFromFormat'];
if (!is_callable($ca)) {
    exit;
}
f($ca);
