<?php
/**
 * @param array{class-string, string}|callable $arg
 */
function foo($arg): void {}

foo([\DateTime::class, "format"]);
