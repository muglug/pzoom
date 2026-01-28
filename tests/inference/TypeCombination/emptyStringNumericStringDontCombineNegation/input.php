<?php
/**
 * @param ""|"hello" $arg
 * @return void
 */
function takesLiteralString($arg) {}

/** @var ""|numeric-string $c */
if (!is_numeric($c)) {
    takesLiteralString($c);
}
