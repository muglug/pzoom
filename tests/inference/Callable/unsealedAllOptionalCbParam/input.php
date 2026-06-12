<?php
/**
 * @param callable(array<string, string>) $arg
 * @return void
 */
function foo($arg) {}

/**
 * @param array{a?: string}&array<string, string> $cb_arg
 * @return void
 */
function bar($cb_arg) {}

foo("bar");
