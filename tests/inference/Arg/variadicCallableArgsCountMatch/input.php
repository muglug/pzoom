<?php
/**
 * @param callable(string, ...int):void $callback
 * @return void
 */
function var_caller($callback) {}

/**
 * @param string $a
 * @param int ...$b
 * @return void
 */
function foo($a, ...$b) {}

var_caller("foo");
