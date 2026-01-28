<?php
/**
 * @param callable(string):void $callback
 * @return void
 */
function caller($callback) {}

/**
 * @param string $a
 * @param string $b
 * @return void
 */
function foo($a, $b) {}

caller("foo");
