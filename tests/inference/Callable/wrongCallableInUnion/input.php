<?php
/**
 * @param int|callable $arg
 */
function foo($arg): void {}

foo([\DateTime::class, "wrongMethod"]);
